//! Fetchers for the EC2-family resource views: EBS volumes, snapshots,
//! security groups, VPCs, subnets, ENIs, EIPs and AMIs.

use chrono::{DateTime, Utc};

use crate::inventory::Warning;

use super::{DetailSection, ResourceList, ResourceRow, age, aws_err, aws_time, dash, sec};

/// Subnet IP pressure. Thresholds are percentages of the subnet's usable
/// capacity (2^(32−prefix) − 5; AWS reserves 5 addresses per subnet) —
/// absolute numbers are meaningless across a /28 vs a /16. Unknown CIDR
/// falls back to absolute counts.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pressure {
    Ok,
    Warn, // ≤ 20% free
    Crit, // ≤ 10% free
}

fn subnet_pressure(cidr: &str, free: i64) -> Pressure {
    let usable = cidr
        .rsplit_once('/')
        .and_then(|(_, p)| p.parse::<u32>().ok())
        .filter(|p| *p <= 32)
        .map(|p| (1i64 << (32 - p)) - 5)
        .filter(|u| *u > 0);
    match usable {
        Some(usable) => {
            let pct = free as f64 / usable as f64;
            if pct <= 0.10 {
                Pressure::Crit
            } else if pct <= 0.20 {
                Pressure::Warn
            } else {
                Pressure::Ok
            }
        }
        None if free < 4 => Pressure::Crit,
        None if free < 16 => Pressure::Warn,
        None => Pressure::Ok,
    }
}

/// Name tag → fallback to the resource id.
fn name_tag(tags: &[aws_sdk_ec2::types::Tag], id: &str) -> String {
    tags.iter()
        .find(|t| t.key() == Some("Name"))
        .and_then(|t| t.value())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| id.to_string())
}

/// The Tags panel (omitted when the resource has no tags).
fn tags_section(tags: &[aws_sdk_ec2::types::Tag]) -> Option<DetailSection> {
    if tags.is_empty() {
        return None;
    }
    let map: std::collections::BTreeMap<&str, &str> = tags
        .iter()
        .map(|t| (t.key().unwrap_or_default(), t.value().unwrap_or_default()))
        .collect();
    Some((
        "Tags".to_string(),
        map.into_iter()
            .map(|(k, v)| (k.to_string(), dash(v)))
            .collect(),
    ))
}

fn push_tags(detail: &mut Vec<DetailSection>, tags: &[aws_sdk_ec2::types::Tag]) {
    if let Some(s) = tags_section(tags) {
        detail.push(s);
    }
}

pub(super) async fn volumes(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    let mut pages = ec2.describe_volumes().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeVolumes",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for v in page.volumes() {
            let id = v.volume_id().unwrap_or_default().to_string();
            let state = v.state().map(|s| s.as_str()).unwrap_or_default();
            let vtype = v.volume_type().map(|t| t.as_str()).unwrap_or("-");
            let attached = v
                .attachments()
                .first()
                .and_then(|a| a.instance_id())
                .unwrap_or("-");
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    name_tag(v.tags(), &id),
                    id.clone(),
                    dash(state),
                    vtype.into(),
                    v.size().map(|s| format!("{s}")).unwrap_or_default(),
                    v.iops().map(|i| format!("{i}")).unwrap_or_default(),
                    attached.to_string(),
                    dash(v.availability_zone().unwrap_or_default()),
                    age(aws_time(v.create_time())),
                ],
                ..Default::default()
            };
            if state == "available" {
                row.warn_cells.push(2); // unattached = billed waste
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("VolumeId", id),
                        ("State", state.into()),
                        ("AZ", v.availability_zone().unwrap_or_default().into()),
                        (
                            "Created",
                            aws_time(v.create_time())
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                        ("AttachedTo", attached.into()),
                    ],
                ),
                sec(
                    "Storage",
                    vec![
                        ("Type", vtype.into()),
                        (
                            "Size (GiB)",
                            v.size().map(|s| s.to_string()).unwrap_or_default(),
                        ),
                        ("Iops", v.iops().map(|i| i.to_string()).unwrap_or_default()),
                        (
                            "Throughput",
                            v.throughput().map(|t| t.to_string()).unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Security",
                    vec![
                        ("Encrypted", v.encrypted().unwrap_or(false).to_string()),
                        ("KmsKeyId", v.kms_key_id().unwrap_or_default().into()),
                    ],
                ),
            ];
            push_tags(&mut row.detail, v.tags());
            res.rows.push(row);
        }
    }
}

pub(super) async fn snapshots(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    // owner self is mandatory: without it this lists every public snapshot
    // in the region (hundreds of thousands of rows).
    let mut pages = ec2
        .describe_snapshots()
        .owner_ids("self")
        .into_paginator()
        .send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeSnapshots",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for s in page.snapshots() {
            let id = s.snapshot_id().unwrap_or_default().to_string();
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    name_tag(s.tags(), &id),
                    id.clone(),
                    s.state().map(|x| x.as_str()).unwrap_or("-").into(),
                    dash(s.progress().unwrap_or_default()),
                    s.volume_size().map(|v| format!("{v}")).unwrap_or_default(),
                    dash(s.volume_id().unwrap_or_default()),
                    if s.encrypted().unwrap_or(false) {
                        "yes"
                    } else {
                        "-"
                    }
                    .into(),
                    age(aws_time(s.start_time())),
                ],
                ..Default::default()
            };
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("SnapshotId", id),
                        ("State", s.state().map(|x| x.as_str()).unwrap_or("-").into()),
                        ("Progress", s.progress().unwrap_or_default().into()),
                        (
                            "Started",
                            aws_time(s.start_time())
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                        ("Description", s.description().unwrap_or_default().into()),
                    ],
                ),
                sec(
                    "Storage",
                    vec![
                        ("VolumeId", s.volume_id().unwrap_or_default().into()),
                        (
                            "Size (GiB)",
                            s.volume_size().map(|v| v.to_string()).unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Security",
                    vec![
                        ("Encrypted", s.encrypted().unwrap_or(false).to_string()),
                        ("KmsKeyId", s.kms_key_id().unwrap_or_default().into()),
                    ],
                ),
            ];
            push_tags(&mut row.detail, s.tags());
            res.rows.push(row);
        }
    }
}

/// One security-group rule as a (label, sources) detail row.
fn rule_row(p: &aws_sdk_ec2::types::IpPermission) -> (String, String) {
    let proto = match p.ip_protocol() {
        Some("-1") | None => "all".to_string(),
        Some(x) => x.to_string(),
    };
    let ports = match (p.from_port(), p.to_port()) {
        (Some(f), Some(t)) if f == t => format!(" {f}"),
        (Some(f), Some(t)) => format!(" {f}–{t}"),
        _ => String::new(),
    };
    let mut sources: Vec<String> = Vec::new();
    sources.extend(
        p.ip_ranges()
            .iter()
            .filter_map(|r| r.cidr_ip())
            .map(String::from),
    );
    sources.extend(
        p.ipv6_ranges()
            .iter()
            .filter_map(|r| r.cidr_ipv6())
            .map(String::from),
    );
    sources.extend(
        p.user_id_group_pairs()
            .iter()
            .filter_map(|g| g.group_id())
            .map(String::from),
    );
    sources.extend(
        p.prefix_list_ids()
            .iter()
            .filter_map(|l| l.prefix_list_id())
            .map(String::from),
    );
    (format!("{proto}{ports}"), sources.join(", "))
}

pub(super) async fn security_groups(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    let mut pages = ec2.describe_security_groups().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeSecurityGroups",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for g in page.security_groups() {
            let id = g.group_id().unwrap_or_default().to_string();
            // group_name is canonical for SGs (not the Name tag)
            let name = dash(g.group_name().unwrap_or_default());
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    if name == "-" {
                        id.clone()
                    } else {
                        name.clone()
                    },
                    id.clone(),
                    dash(g.vpc_id().unwrap_or_default()),
                    format!("{}", g.ip_permissions().len()),
                    format!("{}", g.ip_permissions_egress().len()),
                    g.description().unwrap_or_default().to_string(),
                ],
                ..Default::default()
            };
            row.detail = vec![sec(
                "Details",
                vec![
                    ("GroupId", id),
                    ("GroupName", name),
                    ("VpcId", g.vpc_id().unwrap_or_default().into()),
                    ("Description", g.description().unwrap_or_default().into()),
                ],
            )];
            // the actual rules, console-style (Inbound rules / Outbound rules)
            for (title, perms) in [
                ("Inbound rules", g.ip_permissions()),
                ("Outbound rules", g.ip_permissions_egress()),
            ] {
                if !perms.is_empty() {
                    row.detail
                        .push((title.to_string(), perms.iter().map(rule_row).collect()));
                }
            }
            push_tags(&mut row.detail, g.tags());
            res.rows.push(row);
        }
    }
}

pub(super) async fn vpcs(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    let mut pages = ec2.describe_vpcs().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeVpcs",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for v in page.vpcs() {
            let id = v.vpc_id().unwrap_or_default().to_string();
            let extra = v.cidr_block_association_set().len().saturating_sub(1);
            let mut cidr = v.cidr_block().unwrap_or_default().to_string();
            if extra > 0 {
                cidr.push_str(&format!(" +{extra}"));
            }
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    name_tag(v.tags(), &id),
                    id.clone(),
                    dash(&cidr),
                    v.state().map(|s| s.as_str()).unwrap_or("-").into(),
                    if v.is_default().unwrap_or(false) {
                        "yes"
                    } else {
                        "-"
                    }
                    .into(),
                    v.instance_tenancy()
                        .map(|t| t.as_str())
                        .unwrap_or("-")
                        .into(),
                ],
                ..Default::default()
            };
            let mut networking: Vec<(&str, String)> =
                vec![("CIDR", v.cidr_block().unwrap_or_default().into())];
            let assoc: Vec<String> = v
                .cidr_block_association_set()
                .iter()
                .skip(1)
                .filter_map(|a| a.cidr_block())
                .map(String::from)
                .collect();
            if !assoc.is_empty() {
                networking.push(("Secondary CIDRs", assoc.join(", ")));
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("VpcId", id),
                        ("State", v.state().map(|s| s.as_str()).unwrap_or("-").into()),
                        ("IsDefault", v.is_default().unwrap_or(false).to_string()),
                        (
                            "Tenancy",
                            v.instance_tenancy()
                                .map(|t| t.as_str())
                                .unwrap_or("-")
                                .into(),
                        ),
                    ],
                ),
                sec("Networking", networking),
            ];
            push_tags(&mut row.detail, v.tags());
            res.rows.push(row);
        }
    }
}

pub(super) async fn subnets(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    let mut pages = ec2.describe_subnets().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeSubnets",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for s in page.subnets() {
            let id = s.subnet_id().unwrap_or_default().to_string();
            let free = s.available_ip_address_count().unwrap_or_default();
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    name_tag(s.tags(), &id),
                    id.clone(),
                    dash(s.vpc_id().unwrap_or_default()),
                    dash(s.cidr_block().unwrap_or_default()),
                    dash(s.availability_zone().unwrap_or_default()),
                    format!("{free}"),
                    if s.map_public_ip_on_launch().unwrap_or(false) {
                        "yes"
                    } else {
                        "-"
                    }
                    .into(),
                ],
                ..Default::default()
            };
            match subnet_pressure(s.cidr_block().unwrap_or_default(), i64::from(free)) {
                Pressure::Crit => row.crit_cells.push(5),
                Pressure::Warn => row.warn_cells.push(5),
                Pressure::Ok => {}
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("SubnetId", id),
                        ("AZ", s.availability_zone().unwrap_or_default().into()),
                        (
                            "MapPublicIpOnLaunch",
                            s.map_public_ip_on_launch().unwrap_or(false).to_string(),
                        ),
                    ],
                ),
                sec(
                    "Networking",
                    vec![
                        ("VpcId", s.vpc_id().unwrap_or_default().into()),
                        ("CIDR", s.cidr_block().unwrap_or_default().into()),
                        ("FreeIps", free.to_string()),
                    ],
                ),
            ];
            push_tags(&mut row.detail, s.tags());
            res.rows.push(row);
        }
    }
}

pub(super) async fn enis(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    let mut pages = ec2.describe_network_interfaces().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeNetworkInterfaces",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for n in page.network_interfaces() {
            let id = n.network_interface_id().unwrap_or_default().to_string();
            let status = n.status().map(|s| s.as_str()).unwrap_or_default();
            // AWS-managed ENIs carry no instance id; the requester
            // description ("ELB app/…") says who owns them.
            let attached = n
                .attachment()
                .and_then(|a| a.instance_id())
                .map(str::to_string)
                .unwrap_or_else(|| dash(n.description().unwrap_or_default()));
            let name = {
                let tagged = name_tag(n.tag_set(), &id);
                if tagged == id { id.clone() } else { tagged }
            };
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    name,
                    id.clone(),
                    dash(status),
                    dash(n.private_ip_address().unwrap_or_default()),
                    dash(
                        n.association()
                            .and_then(|a| a.public_ip())
                            .unwrap_or_default(),
                    ),
                    n.interface_type().map(|t| t.as_str()).unwrap_or("-").into(),
                    attached.clone(),
                ],
                ..Default::default()
            };
            if status == "available" {
                row.warn_cells.push(2); // orphan ENI
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("EniId", id),
                        ("Status", status.into()),
                        (
                            "Type",
                            n.interface_type().map(|t| t.as_str()).unwrap_or("-").into(),
                        ),
                        ("AttachedTo", attached),
                        ("Description", n.description().unwrap_or_default().into()),
                    ],
                ),
                sec(
                    "Networking",
                    vec![
                        (
                            "PrivateIp",
                            n.private_ip_address().unwrap_or_default().into(),
                        ),
                        (
                            "PublicIp",
                            n.association()
                                .and_then(|a| a.public_ip())
                                .unwrap_or_default()
                                .into(),
                        ),
                        ("SubnetId", n.subnet_id().unwrap_or_default().into()),
                        ("VpcId", n.vpc_id().unwrap_or_default().into()),
                    ],
                ),
            ];
            push_tags(&mut row.detail, n.tag_set());
            res.rows.push(row);
        }
    }
}

pub(super) async fn eips(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    // No paginator exists for DescribeAddresses (per-region EIP quotas are
    // tiny), so a single call is the whole listing.
    let out = match ec2.describe_addresses().send().await {
        Ok(o) => o,
        Err(e) => {
            res.warnings.push(Warning {
                op: "ec2:DescribeAddresses",
                err: aws_err(&e),
            });
            return;
        }
    };
    for a in out.addresses() {
        let id = a
            .allocation_id()
            .unwrap_or_else(|| a.public_ip().unwrap_or_default())
            .to_string();
        let associated = a
            .instance_id()
            .or(a.network_interface_id())
            .unwrap_or("-")
            .to_string();
        let mut row = ResourceRow {
            id: id.clone(),
            cells: vec![
                name_tag(a.tags(), &id),
                dash(a.public_ip().unwrap_or_default()),
                dash(a.allocation_id().unwrap_or_default()),
                associated.clone(),
                dash(a.private_ip_address().unwrap_or_default()),
            ],
            ..Default::default()
        };
        if associated == "-" {
            row.warn_cells.push(3); // unassociated EIPs are billed
        }
        row.detail = vec![
            sec(
                "Details",
                vec![
                    ("PublicIp", a.public_ip().unwrap_or_default().into()),
                    ("AllocationId", a.allocation_id().unwrap_or_default().into()),
                ],
            ),
            sec(
                "Networking",
                vec![
                    ("AssociatedTo", associated),
                    (
                        "PrivateIp",
                        a.private_ip_address().unwrap_or_default().into(),
                    ),
                ],
            ),
        ];
        push_tags(&mut row.detail, a.tags());
        res.rows.push(row);
    }
}

pub(super) async fn amis(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
    // owners self is mandatory: without it this lists every public AMI.
    let mut pages = ec2.describe_images().owners("self").into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeImages",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for i in page.images() {
            let id = i.image_id().unwrap_or_default().to_string();
            // creation_date is an ISO-8601 string, not a DateTime
            let created = i
                .creation_date()
                .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                .map(|d| d.with_timezone(&Utc));
            let name = dash(i.name().unwrap_or_default());
            let mut row = ResourceRow {
                id: id.clone(),
                cells: vec![
                    if name == "-" {
                        id.clone()
                    } else {
                        name.clone()
                    },
                    id.clone(),
                    i.state().map(|s| s.as_str()).unwrap_or("-").into(),
                    i.architecture().map(|a| a.as_str()).unwrap_or("-").into(),
                    dash(i.platform_details().unwrap_or_default()),
                    if i.public().unwrap_or(false) {
                        "yes"
                    } else {
                        "-"
                    }
                    .into(),
                    age(created),
                ],
                ..Default::default()
            };
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("ImageId", id),
                        ("Name", name),
                        ("State", i.state().map(|s| s.as_str()).unwrap_or("-").into()),
                        (
                            "Architecture",
                            i.architecture().map(|a| a.as_str()).unwrap_or("-").into(),
                        ),
                        ("Platform", i.platform_details().unwrap_or_default().into()),
                        ("Created", i.creation_date().unwrap_or_default().into()),
                        ("Description", i.description().unwrap_or_default().into()),
                    ],
                ),
                sec(
                    "Security",
                    vec![
                        ("Public", i.public().unwrap_or(false).to_string()),
                        ("OwnerId", i.owner_id().unwrap_or_default().into()),
                    ],
                ),
            ];
            push_tags(&mut row.detail, i.tags());
            res.rows.push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_pressure_is_relative_to_capacity() {
        // /28 → 11 usable: 2 free = 18% → warn; 1 free = 9% → critical
        assert_eq!(subnet_pressure("10.1.9.0/28", 2), Pressure::Warn);
        assert_eq!(subnet_pressure("10.1.9.0/28", 1), Pressure::Crit);
        // /28 with 5 free = 45% → fine, even though the absolute is small
        assert_eq!(subnet_pressure("10.1.9.0/28", 5), Pressure::Ok);
        // /16 → 65531 usable: 200 free = 0.3% → critical despite "200 free"
        assert_eq!(subnet_pressure("10.0.0.0/16", 200), Pressure::Crit);
        // /24 → 251 usable: 40 free = 16% → warn
        assert_eq!(subnet_pressure("10.0.1.0/24", 40), Pressure::Warn);
        assert_eq!(subnet_pressure("10.0.1.0/24", 180), Pressure::Ok);
        // unparseable CIDR falls back to absolutes
        assert_eq!(subnet_pressure("", 2), Pressure::Crit);
        assert_eq!(subnet_pressure("bogus", 10), Pressure::Warn);
        assert_eq!(subnet_pressure("bogus", 100), Pressure::Ok);
    }
}
