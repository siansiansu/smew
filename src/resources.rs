//! The non-EC2-instance resource views behind the `:` command: EBS volumes,
//! snapshots, security groups, VPCs, subnets, ENIs, EIPs and AMIs.
//!
//! Deliberately a "simple table" track: every resource is one paginated
//! describe call mapped to display cells + a key/value detail dump. The
//! rich Instance path (sessions, metrics, marks) stays in inventory.rs.
//! API failures degrade to Warnings, like the instance list.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::inventory::Warning;

/// Which table the main panel shows. Instances is the featureful default;
/// the rest render through the generic resource table.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResourceKind {
    #[default]
    Instances,
    Volumes,
    Snapshots,
    SecurityGroups,
    Vpcs,
    Subnets,
    Enis,
    Eips,
    Amis,
}

/// All non-instance kinds (registry order = suggestion order).
pub const KINDS: [ResourceKind; 8] = [
    ResourceKind::Volumes,
    ResourceKind::Snapshots,
    ResourceKind::SecurityGroups,
    ResourceKind::Vpcs,
    ResourceKind::Subnets,
    ResourceKind::Enis,
    ResourceKind::Eips,
    ResourceKind::Amis,
];

impl ResourceKind {
    /// The canonical short name shown in the panel title and crumbs.
    pub fn title(self) -> &'static str {
        match self {
            ResourceKind::Instances => "ec2",
            ResourceKind::Volumes => "vol",
            ResourceKind::Snapshots => "snap",
            ResourceKind::SecurityGroups => "sg",
            ResourceKind::Vpcs => "vpc",
            ResourceKind::Subnets => "subnet",
            ResourceKind::Enis => "eni",
            ResourceKind::Eips => "eip",
            ResourceKind::Amis => "ami",
        }
    }

    /// AWS-CLI-conventional command aliases (canonical first).
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            ResourceKind::Instances => &["ec2", "instances", "i"],
            ResourceKind::Volumes => &["vol", "ebs", "volumes"],
            ResourceKind::Snapshots => &["snap", "snapshots"],
            ResourceKind::SecurityGroups => &["sg", "securitygroups"],
            ResourceKind::Vpcs => &["vpc", "vpcs"],
            ResourceKind::Subnets => &["subnet", "sub", "subnets"],
            ResourceKind::Enis => &["eni", "networkinterfaces"],
            ResourceKind::Eips => &["eip", "addresses"],
            ResourceKind::Amis => &["ami", "images"],
        }
    }

    /// Resolves a typed command word to a kind.
    pub fn from_alias(word: &str) -> Option<ResourceKind> {
        std::iter::once(ResourceKind::Instances)
            .chain(KINDS)
            .find(|k| k.aliases().contains(&word))
    }

    /// Column titles of the generic table (widths auto-fit to content).
    /// Kinds whose API exposes no creation time simply have no AGE column.
    pub fn columns(self) -> &'static [&'static str] {
        match self {
            ResourceKind::Instances => &[],
            ResourceKind::Volumes => &[
                "NAME",
                "VOLUME-ID",
                "STATE",
                "TYPE",
                "SIZE",
                "IOPS",
                "ATTACHED-TO",
                "AZ",
                "AGE",
            ],
            ResourceKind::Snapshots => &[
                "NAME",
                "SNAPSHOT-ID",
                "STATE",
                "PROGRESS",
                "SIZE",
                "VOLUME-ID",
                "ENC",
                "AGE",
            ],
            ResourceKind::SecurityGroups => &[
                "NAME",
                "GROUP-ID",
                "VPC-ID",
                "INGRESS",
                "EGRESS",
                "DESCRIPTION",
            ],
            ResourceKind::Vpcs => &["NAME", "VPC-ID", "CIDR", "STATE", "DEFAULT", "TENANCY"],
            ResourceKind::Subnets => &[
                "NAME",
                "SUBNET-ID",
                "VPC-ID",
                "CIDR",
                "AZ",
                "FREE-IPS",
                "PUBLIC",
            ],
            ResourceKind::Enis => &[
                "NAME",
                "ENI-ID",
                "STATUS",
                "PRIVATE-IP",
                "PUBLIC-IP",
                "TYPE",
                "ATTACHED-TO",
            ],
            ResourceKind::Eips => &[
                "NAME",
                "PUBLIC-IP",
                "ALLOCATION-ID",
                "ASSOCIATED-TO",
                "PRIVATE-IP",
            ],
            ResourceKind::Amis => &[
                "NAME", "AMI-ID", "STATE", "ARCH", "PLATFORM", "PUBLIC", "AGE",
            ],
        }
    }

    /// Whether Enter on a row drills into the ec2 view filtered by the row's
    /// id (VPC/subnet/SG contain instances; the rest open the detail).
    pub fn drills_to_instances(self) -> bool {
        matches!(
            self,
            ResourceKind::Vpcs | ResourceKind::Subnets | ResourceKind::SecurityGroups
        )
    }
}

/// One row of a generic resource table. cells aligns with kind.columns();
/// warn/crit mark cells for orange/red highlighting (orphaned volumes,
/// unassociated EIPs, low FREE-IPS — the "billed waste" signals).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceRow {
    pub id: String,
    pub cells: Vec<String>,
    pub warn_cells: Vec<usize>,
    pub crit_cells: Vec<usize>,
    /// Full key/value record for the detail overlay (includes tags).
    pub detail: Vec<(String, String)>,
}

/// The outcome of a resource list call.
#[derive(Debug, Default)]
pub struct ResourceList {
    pub kind: ResourceKind,
    pub rows: Vec<ResourceRow>,
    pub warnings: Vec<Warning>,
}

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

fn aws_err(e: &(impl std::error::Error + 'static)) -> String {
    format!("{}", aws_sdk_ec2::error::DisplayErrorContext(e))
}

/// Compact relative age, "-" when the API exposes no creation time.
fn age(t: Option<DateTime<Utc>>) -> String {
    let Some(t) = t else {
        return "-".to_string();
    };
    let s = crate::tui::age_label(Some(t));
    if s.is_empty() { "-".to_string() } else { s }
}

fn aws_time(t: Option<&aws_sdk_ec2::primitives::DateTime>) -> Option<DateTime<Utc>> {
    t.and_then(|t| DateTime::from_timestamp(t.secs(), t.subsec_nanos()))
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

fn tag_pairs(tags: &[aws_sdk_ec2::types::Tag]) -> Vec<(String, String)> {
    let map: BTreeMap<&str, &str> = tags
        .iter()
        .map(|t| (t.key().unwrap_or_default(), t.value().unwrap_or_default()))
        .collect();
    map.into_iter()
        .map(|(k, v)| (format!("tag:{k}"), v.to_string()))
        .collect()
}

fn dash(s: &str) -> String {
    if s.is_empty() {
        "-".to_string()
    } else {
        s.to_string()
    }
}

/// Fetches one resource kind from AWS. Failures come back as Warnings with
/// however many rows were listed before the error (usually none).
pub(crate) async fn list_aws(ec2: &aws_sdk_ec2::Client, kind: ResourceKind) -> ResourceList {
    let mut res = ResourceList {
        kind,
        ..Default::default()
    };
    match kind {
        ResourceKind::Instances => {} // handled by inventory::list
        ResourceKind::Volumes => volumes(ec2, &mut res).await,
        ResourceKind::Snapshots => snapshots(ec2, &mut res).await,
        ResourceKind::SecurityGroups => security_groups(ec2, &mut res).await,
        ResourceKind::Vpcs => vpcs(ec2, &mut res).await,
        ResourceKind::Subnets => subnets(ec2, &mut res).await,
        ResourceKind::Enis => enis(ec2, &mut res).await,
        ResourceKind::Eips => eips(ec2, &mut res).await,
        ResourceKind::Amis => amis(ec2, &mut res).await,
    }
    res.rows
        .sort_by(|a, b| (a.cells.first(), &a.id).cmp(&(b.cells.first(), &b.id)));
    res
}

async fn volumes(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
                    v.volume_type().map(|t| t.as_str()).unwrap_or("-").into(),
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
                ("VolumeId".into(), id),
                ("State".into(), state.into()),
                (
                    "Type".into(),
                    v.volume_type().map(|t| t.as_str()).unwrap_or("-").into(),
                ),
                (
                    "Size (GiB)".into(),
                    v.size().map(|s| s.to_string()).unwrap_or_default(),
                ),
                (
                    "Iops".into(),
                    v.iops().map(|i| i.to_string()).unwrap_or_default(),
                ),
                (
                    "Encrypted".into(),
                    v.encrypted().unwrap_or(false).to_string(),
                ),
                ("AttachedTo".into(), attached.into()),
                (
                    "AZ".into(),
                    v.availability_zone().unwrap_or_default().into(),
                ),
                (
                    "Created".into(),
                    aws_time(v.create_time())
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default(),
                ),
            ];
            row.detail.extend(tag_pairs(v.tags()));
            res.rows.push(row);
        }
    }
}

async fn snapshots(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
                ("SnapshotId".into(), id),
                (
                    "State".into(),
                    s.state().map(|x| x.as_str()).unwrap_or("-").into(),
                ),
                ("Progress".into(), s.progress().unwrap_or_default().into()),
                (
                    "Size (GiB)".into(),
                    s.volume_size().map(|v| v.to_string()).unwrap_or_default(),
                ),
                ("VolumeId".into(), s.volume_id().unwrap_or_default().into()),
                (
                    "Encrypted".into(),
                    s.encrypted().unwrap_or(false).to_string(),
                ),
                (
                    "Description".into(),
                    s.description().unwrap_or_default().into(),
                ),
                (
                    "Started".into(),
                    aws_time(s.start_time())
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default(),
                ),
            ];
            row.detail.extend(tag_pairs(s.tags()));
            res.rows.push(row);
        }
    }
}

async fn security_groups(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
            row.detail = vec![
                ("GroupId".into(), id),
                ("GroupName".into(), name),
                ("VpcId".into(), g.vpc_id().unwrap_or_default().into()),
                ("IngressRules".into(), g.ip_permissions().len().to_string()),
                (
                    "EgressRules".into(),
                    g.ip_permissions_egress().len().to_string(),
                ),
                (
                    "Description".into(),
                    g.description().unwrap_or_default().into(),
                ),
            ];
            row.detail.extend(tag_pairs(g.tags()));
            res.rows.push(row);
        }
    }
}

async fn vpcs(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
            row.detail = vec![
                ("VpcId".into(), id),
                ("Cidr".into(), cidr),
                (
                    "State".into(),
                    v.state().map(|s| s.as_str()).unwrap_or("-").into(),
                ),
                (
                    "IsDefault".into(),
                    v.is_default().unwrap_or(false).to_string(),
                ),
                (
                    "Tenancy".into(),
                    v.instance_tenancy()
                        .map(|t| t.as_str())
                        .unwrap_or("-")
                        .into(),
                ),
            ];
            row.detail.extend(tag_pairs(v.tags()));
            res.rows.push(row);
        }
    }
}

async fn subnets(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
                ("SubnetId".into(), id),
                ("VpcId".into(), s.vpc_id().unwrap_or_default().into()),
                ("Cidr".into(), s.cidr_block().unwrap_or_default().into()),
                (
                    "AZ".into(),
                    s.availability_zone().unwrap_or_default().into(),
                ),
                ("FreeIps".into(), free.to_string()),
                (
                    "MapPublicIpOnLaunch".into(),
                    s.map_public_ip_on_launch().unwrap_or(false).to_string(),
                ),
            ];
            row.detail.extend(tag_pairs(s.tags()));
            res.rows.push(row);
        }
    }
}

async fn enis(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
                ("EniId".into(), id),
                ("Status".into(), status.into()),
                (
                    "PrivateIp".into(),
                    n.private_ip_address().unwrap_or_default().into(),
                ),
                (
                    "PublicIp".into(),
                    n.association()
                        .and_then(|a| a.public_ip())
                        .unwrap_or_default()
                        .into(),
                ),
                (
                    "Type".into(),
                    n.interface_type().map(|t| t.as_str()).unwrap_or("-").into(),
                ),
                ("AttachedTo".into(), attached),
                ("SubnetId".into(), n.subnet_id().unwrap_or_default().into()),
                ("VpcId".into(), n.vpc_id().unwrap_or_default().into()),
                (
                    "Description".into(),
                    n.description().unwrap_or_default().into(),
                ),
            ];
            row.detail.extend(tag_pairs(n.tag_set()));
            res.rows.push(row);
        }
    }
}

async fn eips(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
            ("PublicIp".into(), a.public_ip().unwrap_or_default().into()),
            (
                "AllocationId".into(),
                a.allocation_id().unwrap_or_default().into(),
            ),
            ("AssociatedTo".into(), associated),
            (
                "PrivateIp".into(),
                a.private_ip_address().unwrap_or_default().into(),
            ),
        ];
        row.detail.extend(tag_pairs(a.tags()));
        res.rows.push(row);
    }
}

async fn amis(ec2: &aws_sdk_ec2::Client, res: &mut ResourceList) {
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
                ("ImageId".into(), id),
                ("Name".into(), name),
                (
                    "State".into(),
                    i.state().map(|s| s.as_str()).unwrap_or("-").into(),
                ),
                (
                    "Architecture".into(),
                    i.architecture().map(|a| a.as_str()).unwrap_or("-").into(),
                ),
                (
                    "Platform".into(),
                    i.platform_details().unwrap_or_default().into(),
                ),
                ("Public".into(), i.public().unwrap_or(false).to_string()),
                (
                    "Description".into(),
                    i.description().unwrap_or_default().into(),
                ),
                (
                    "Created".into(),
                    i.creation_date().unwrap_or_default().into(),
                ),
            ];
            row.detail.extend(tag_pairs(i.tags()));
            res.rows.push(row);
        }
    }
}

// ---- developer-mode fixtures ----

/// Dev-mode rows, referentially consistent with inventory::mock_instances
/// (same vpc/subnet/sg/instance ids) so drill-down navigation works offline.
/// Each kind carries its edge cases: orphaned/errored volumes, a stuck and
/// a zero-progress snapshot, AWS-managed ENIs (NAT/endpoint/Lambda), idle
/// EIPs, subnets near IP exhaustion, and a failed AMI bake.
pub(crate) fn mock(kind: ResourceKind) -> Vec<ResourceRow> {
    let row = |id: &str, cells: &[&str]| ResourceRow {
        id: id.to_string(),
        cells: cells.iter().map(|s| s.to_string()).collect(),
        detail: vec![("Id".into(), id.to_string())]
            .into_iter()
            .chain(
                kind.columns()
                    .iter()
                    .zip(cells.iter())
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            )
            .collect(),
        ..Default::default()
    };
    // Cell-mark helpers: waste signals go orange, error states go red.
    let warn = |mut r: ResourceRow, c: usize| {
        r.warn_cells.push(c);
        r
    };
    let crit = |mut r: ResourceRow, c: usize| {
        r.crit_cells.push(c);
        r
    };
    #[rustfmt::skip]
    let mut rows = match kind {
        ResourceKind::Instances => Vec::new(),
        ResourceKind::Volumes => vec![
            row("vol-0aaa1",  &["web-01-root",     "vol-0aaa1",  "in-use",    "gp3",      "20",   "3000",  "i-0aaaaaaaaaaaaaa01", "ap-northeast-1a", "30d"]),
            row("vol-0aaa2",  &["web-02-root",     "vol-0aaa2",  "in-use",    "gp3",      "20",   "3000",  "i-0aaaaaaaaaaaaaa02", "ap-northeast-1c", "30d"]),
            warn(row("vol-0bbb2", &["orphan-data", "vol-0bbb2",  "available", "gp3",      "100",  "3000",  "-",                   "ap-northeast-1c", "90d"]), 2),
            row("vol-0ccc3",  &["db-fast",         "vol-0ccc3",  "in-use",    "io2",      "500",  "10000", "i-0dbdbdbdbdbdbdb01", "ap-northeast-1a", "60d"]),
            row("vol-0ccc4",  &["db-primary-logs", "vol-0ccc4",  "in-use",    "gp3",      "100",  "3000",  "i-0dbdbdbdbdbdbdb01", "ap-northeast-1a", "60d"]),
            row("vol-0cafe1", &["kafka-01-data",   "vol-0cafe1", "in-use",    "st1",      "1000", "-",     "i-0cafecafecafeca01", "ap-northeast-1a", "150d"]),
            row("vol-0cafe2", &["kafka-02-data",   "vol-0cafe2", "in-use",    "st1",      "1000", "-",     "i-0cafecafecafeca02", "ap-northeast-1c", "150d"]),
            warn(row("vol-0old9", &["ancient-standard", "vol-0old9", "available", "standard", "8", "-",    "-",                   "ap-northeast-1a", "1500d"]), 2),
            crit(row("vol-0err8", &["stuck-error",  "vol-0err8", "error",     "gp2",      "50",   "150",   "-",                   "ap-northeast-1c", "200d"]), 2),
            row("vol-0feed5", &["gpu-scratch",     "vol-0feed5", "in-use",    "gp3",      "400",  "12000", "i-0feedfacefeedfa01", "ap-northeast-1a", "3d"]),
            row("vol-0c1c6",  &["ci-cache",        "vol-0c1c6",  "in-use",    "gp2",      "120",  "360",   "i-0c1c1c1c1c1c1c101", "ap-northeast-1a", "5d"]),
        ],
        ResourceKind::Snapshots => vec![
            row("snap-0aaa1",  &["daily-web",      "snap-0aaa1",  "completed", "100%", "20",   "vol-0aaa1",  "yes", "12h"]),
            row("snap-0bbb2",  &["migration",      "snap-0bbb2",  "pending",   "43%",  "100",  "vol-0bbb2",  "-",   "5m"]),
            row("snap-0ccc3",  &["ancient-backup", "snap-0ccc3",  "completed", "100%", "8",    "vol-0ddd4",  "-",   "400d"]),
            row("snap-0db01",  &["db-nightly",     "snap-0db01",  "completed", "100%", "500",  "vol-0ccc3",  "yes", "8h"]),
            row("snap-0cafe1", &["kafka-weekly",   "snap-0cafe1", "completed", "100%", "1000", "vol-0cafe1", "-",   "3d"]),
            crit(row("snap-0err1", &["stuck-copy", "snap-0err1",  "error",     "55%",  "50",   "vol-0err8",  "-",   "30d"]), 2),
            row("snap-0new1",  &["fresh-start",    "snap-0new1",  "pending",   "0%",   "20",   "vol-0aaa2",  "yes", "30s"]),
            row("snap-0gold1", &["golden-base",    "snap-0gold1", "completed", "100%", "30",   "vol-0old9",  "-",   "800d"]),
        ],
        ResourceKind::SecurityGroups => vec![
            row("sg-0dev00000000dev0", &["dev-default",    "sg-0dev00000000dev0", "vpc-0dev00000000dev0", "3",  "1", "default dev security group"]),
            row("sg-0web00000000web0", &["web-sg",         "sg-0web00000000web0", "vpc-0dev00000000dev0", "2",  "1", "web tier: 80/443 from the ALB only"]),
            row("sg-0db000000000db00", &["db-sg",          "sg-0db000000000db00", "vpc-0dev00000000dev0", "2",  "1", "postgres 5432 from app tiers only"]),
            row("sg-0lock1",           &["locked-down",    "sg-0lock1",           "vpc-0dev00000000dev0", "0",  "1", "no ingress at all"]),
            row("sg-0wide2",           &["legacy-wide-open", "sg-0wide2",         "vpc-0sec00000000sec0", "14", "3", "inherited from the old account, needs a serious audit before anyone touches it"]),
            row("sg-0prd00000000prd0", &["prod-legacy-sg", "sg-0prd00000000prd0", "vpc-0prd00000000prd0", "8",  "2", "prod legacy app + rdp"]),
        ],
        ResourceKind::Vpcs => vec![
            row("vpc-0dev00000000dev0", &["dev-main",      "vpc-0dev00000000dev0", "10.0.0.0/16",    "available", "yes", "default"]),
            row("vpc-0sec00000000sec0", &["dev-secondary", "vpc-0sec00000000sec0", "10.1.0.0/16 +1", "available", "-",   "default"]),
            row("vpc-0prd00000000prd0", &["prod-legacy",   "vpc-0prd00000000prd0", "10.9.0.0/16",    "available", "-",   "default"]),
        ],
        ResourceKind::Subnets => vec![
            row("subnet-0dev0000000000a",  &["dev-a-public",  "subnet-0dev0000000000a",  "vpc-0dev00000000dev0", "10.0.1.0/24", "ap-northeast-1a", "180", "yes"]),
            row("subnet-0dev0000000000c",  &["dev-c-private", "subnet-0dev0000000000c",  "vpc-0dev00000000dev0", "10.0.2.0/24", "ap-northeast-1c", "200", "-"]),
            // /28 → 11 usable; 1 free = 9% → the critical rendering path
            crit(row("subnet-0full0000000000f", &["dev-a-crowded", "subnet-0full0000000000f", "vpc-0sec00000000sec0", "10.1.9.0/28", "ap-northeast-1a", "1", "-"]), 5),
            row("subnet-0sec0000000000a",  &["sec-a-public",  "subnet-0sec0000000000a",  "vpc-0sec00000000sec0", "10.1.1.0/24", "ap-northeast-1a", "240", "yes"]),
            // /24 → 251 usable; 30 free = 12% → the warn path
            warn(row("subnet-0prd0000000000a", &["prod-a",    "subnet-0prd0000000000a",  "vpc-0prd00000000prd0", "10.9.1.0/24", "ap-northeast-1a", "30",  "-"]), 5),
            row("subnet-0prd0000000000c",  &["prod-c",        "subnet-0prd0000000000c",  "vpc-0prd00000000prd0", "10.9.2.0/24", "ap-northeast-1c", "210", "-"]),
        ],
        ResourceKind::Enis => vec![
            row("eni-0aaa1",  &["web-01-eth0",     "eni-0aaa1",  "in-use",    "10.0.1.11",  "-",             "interface",             "i-0aaaaaaaaaaaaaa01"]),
            row("eni-0win2",  &["windows-eth0",    "eni-0win2",  "in-use",    "10.0.6.14",  "203.0.113.20",  "interface",             "i-0abcdefabcdefab01"]),
            warn(row("eni-0orp3", &["eni-0orp3",   "eni-0orp3",  "available", "10.0.2.99",  "-",             "interface",             "-"]), 2),
            row("eni-0elb4",  &["eni-0elb4",       "eni-0elb4",  "in-use",    "10.0.1.200", "-",             "network_load_balancer", "ELB app/dev-alb/50dc6c495c0c9188"]),
            row("eni-0cafe5", &["kafka-01-eth0",   "eni-0cafe5", "in-use",    "10.0.7.11",  "-",             "interface",             "i-0cafecafecafeca01"]),
            row("eni-0db06",  &["db-primary-eth0", "eni-0db06",  "in-use",    "10.0.3.11",  "-",             "interface",             "i-0dbdbdbdbdbdbdb01"]),
            row("eni-0nat7",  &["eni-0nat7",       "eni-0nat7",  "in-use",    "10.0.1.250", "203.0.113.1",   "nat_gateway",           "NAT Gateway nat-0dev0001"]),
            row("eni-0vpce8", &["eni-0vpce8",      "eni-0vpce8", "in-use",    "10.0.2.201", "-",             "vpc_endpoint",          "VPC Endpoint vpce-0dev-ssm"]),
            row("eni-0lam9",  &["eni-0lam9",       "eni-0lam9",  "in-use",    "10.0.2.202", "-",             "lambda",                "AWS Lambda VPC dev-cleanup-fn"]),
            row("eni-0feed0", &["gpu-train-eth0",  "eni-0feed0", "in-use",    "10.0.10.5",  "203.0.113.55",  "interface",             "i-0feedfacefeedfa01"]),
        ],
        ResourceKind::Eips => vec![
            row("eipalloc-0win1",  &["windows-01-eip",  "203.0.113.20",  "eipalloc-0win1",  "i-0abcdefabcdefab01", "10.0.6.14"]),
            warn(row("eipalloc-0idle2", &["forgotten-eip", "203.0.113.99", "eipalloc-0idle2", "-",                "-"]), 3),
            row("eipalloc-0nat3",  &["nat-gw-eip",      "203.0.113.1",   "eipalloc-0nat3",  "eni-0nat7",           "10.0.1.250"]),
            row("eipalloc-0gpu4",  &["gpu-train-eip",   "203.0.113.55",  "eipalloc-0gpu4",  "i-0feedfacefeedfa01", "10.0.10.5"]),
            warn(row("eipalloc-0idle5", &["forgotten-eip-2", "203.0.113.120", "eipalloc-0idle5", "-",              "-"]), 3),
        ],
        ResourceKind::Amis => vec![
            row("ami-0base1", &["dev-base-al2023",    "ami-0base1", "available", "x86_64", "Linux/UNIX", "-",   "60d"]),
            row("ami-0arm2",  &["dev-base-arm",       "ami-0arm2",  "available", "arm64",  "Linux/UNIX", "-",   "14d"]),
            row("ami-0bake3", &["nightly-bake",       "ami-0bake3", "pending",   "x86_64", "Linux/UNIX", "-",   "10m"]),
            row("ami-0win4",  &["windows-base-2022",  "ami-0win4",  "available", "x86_64", "Windows",    "-",   "90d"]),
            row("ami-0gold5", &["golden-2023-public", "ami-0gold5", "available", "x86_64", "Linux/UNIX", "yes", "500d"]),
            crit(row("ami-0fail6", &["failed-bake",   "ami-0fail6", "failed",    "x86_64", "Linux/UNIX", "-",   "1d"]), 2),
        ],
    };
    rows.sort_by(|a, b| {
        (a.cells.first().cloned(), a.id.clone()).cmp(&(b.cells.first().cloned(), b.id.clone()))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_and_are_unique() {
        assert_eq!(
            ResourceKind::from_alias("sg"),
            Some(ResourceKind::SecurityGroups)
        );
        assert_eq!(ResourceKind::from_alias("ebs"), Some(ResourceKind::Volumes));
        assert_eq!(
            ResourceKind::from_alias("ec2"),
            Some(ResourceKind::Instances)
        );
        assert_eq!(ResourceKind::from_alias("nope"), None);
        // no alias maps to two kinds
        let mut seen = std::collections::HashSet::new();
        for k in std::iter::once(ResourceKind::Instances).chain(KINDS) {
            for a in k.aliases() {
                assert!(seen.insert(*a), "duplicate alias {a}");
            }
        }
    }

    #[test]
    fn mock_rows_match_column_arity_and_are_sorted() {
        for k in KINDS {
            let rows = mock(k);
            assert!(!rows.is_empty(), "{k:?} needs fixtures");
            let n = k.columns().len();
            for r in &rows {
                assert_eq!(r.cells.len(), n, "{k:?} row {} arity", r.id);
                assert!(!r.detail.is_empty(), "{k:?} row {} detail", r.id);
                for &w in r.warn_cells.iter().chain(&r.crit_cells) {
                    assert!(w < n, "{k:?} row {} marks column {w} out of range", r.id);
                }
            }
            let names: Vec<_> = rows.iter().map(|r| r.cells[0].clone()).collect();
            let mut sorted = names.clone();
            sorted.sort();
            assert_eq!(names, sorted, "{k:?} must be name-sorted");
        }
    }

    #[test]
    fn mock_fixtures_reference_mock_instances() {
        // Drill-down offline depends on these ids lining up with
        // inventory::mock_instances.
        let sg = mock(ResourceKind::SecurityGroups);
        assert!(sg.iter().any(|r| r.id == "sg-0dev00000000dev0"));
        let vpc = mock(ResourceKind::Vpcs);
        assert!(vpc.iter().any(|r| r.id == "vpc-0dev00000000dev0"));
        let subnet = mock(ResourceKind::Subnets);
        assert!(subnet.iter().any(|r| r.id == "subnet-0dev0000000000a"));
        let eni = mock(ResourceKind::Enis);
        assert!(
            eni.iter()
                .any(|r| r.cells.contains(&"i-0aaaaaaaaaaaaaa01".to_string()))
        );
    }

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

    #[test]
    fn waste_signals_are_marked() {
        let vol = mock(ResourceKind::Volumes);
        assert!(
            vol.iter().any(|r| !r.warn_cells.is_empty()),
            "an available volume must warn"
        );
        let eip = mock(ResourceKind::Eips);
        assert!(
            eip.iter().any(|r| !r.warn_cells.is_empty()),
            "an unassociated eip must warn"
        );
        let subnet = mock(ResourceKind::Subnets);
        assert!(
            subnet.iter().any(|r| !r.crit_cells.is_empty()),
            "a nearly-full subnet must be critical"
        );
    }
}
