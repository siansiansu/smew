//! The non-EC2-instance resource views behind the `:` command — the rest of
//! the EC2 family (volumes, security groups, VPCs, …) plus the other AWS
//! services (S3, Lambda, RDS, …).
//!
//! Deliberately a "simple table" track: every resource is one paginated
//! list/describe call mapped to display cells + a grouped key/value record
//! for the describe dashboard. The rich Instance path (sessions, metrics,
//! marks) stays in inventory.rs. API failures degrade to Warnings, like the
//! instance list.

mod ec2;
mod mock;
mod services;

pub(crate) use mock::mock;

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
    S3,
    Lambda,
    Asg,
    Rds,
    Dynamo,
    Elb,
    Sqs,
    Sns,
    Ecs,
    Eks,
    Cfn,
}

/// All non-instance kinds (registry order = suggestion order, grouped by
/// category).
pub const KINDS: [ResourceKind; 19] = [
    ResourceKind::Volumes,
    ResourceKind::Snapshots,
    ResourceKind::SecurityGroups,
    ResourceKind::Vpcs,
    ResourceKind::Subnets,
    ResourceKind::Enis,
    ResourceKind::Eips,
    ResourceKind::Amis,
    ResourceKind::S3,
    ResourceKind::Lambda,
    ResourceKind::Asg,
    ResourceKind::Rds,
    ResourceKind::Dynamo,
    ResourceKind::Elb,
    ResourceKind::Sqs,
    ResourceKind::Sns,
    ResourceKind::Ecs,
    ResourceKind::Eks,
    ResourceKind::Cfn,
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
            ResourceKind::S3 => "s3",
            ResourceKind::Lambda => "lambda",
            ResourceKind::Asg => "asg",
            ResourceKind::Rds => "rds",
            ResourceKind::Dynamo => "ddb",
            ResourceKind::Elb => "elb",
            ResourceKind::Sqs => "sqs",
            ResourceKind::Sns => "sns",
            ResourceKind::Ecs => "ecs",
            ResourceKind::Eks => "eks",
            ResourceKind::Cfn => "cfn",
        }
    }

    /// The AWS official product category the kind belongs to (groups the
    /// help page and the command suggestions).
    pub fn category(self) -> &'static str {
        match self {
            ResourceKind::Instances
            | ResourceKind::Amis
            | ResourceKind::Lambda
            | ResourceKind::Asg => "Compute",
            ResourceKind::Volumes | ResourceKind::Snapshots | ResourceKind::S3 => "Storage",
            ResourceKind::Rds | ResourceKind::Dynamo => "Database",
            ResourceKind::SecurityGroups => "Security, Identity & Compliance",
            ResourceKind::Vpcs
            | ResourceKind::Subnets
            | ResourceKind::Enis
            | ResourceKind::Eips
            | ResourceKind::Elb => "Networking & Content Delivery",
            ResourceKind::Sqs | ResourceKind::Sns => "Application Integration",
            ResourceKind::Ecs | ResourceKind::Eks => "Containers",
            ResourceKind::Cfn => "Management & Governance",
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
            ResourceKind::S3 => &["s3", "buckets"],
            ResourceKind::Lambda => &["lambda", "fn", "functions"],
            ResourceKind::Asg => &["asg", "autoscaling"],
            ResourceKind::Rds => &["rds", "db"],
            ResourceKind::Dynamo => &["ddb", "dynamodb", "tables"],
            ResourceKind::Elb => &["elb", "lb", "alb", "loadbalancers"],
            ResourceKind::Sqs => &["sqs", "queues"],
            ResourceKind::Sns => &["sns", "topics"],
            ResourceKind::Ecs => &["ecs"],
            ResourceKind::Eks => &["eks"],
            ResourceKind::Cfn => &["cfn", "stacks", "cloudformation"],
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
            ResourceKind::S3 => &["NAME", "REGION", "AGE"],
            ResourceKind::Lambda => &["NAME", "RUNTIME", "MEMORY", "TIMEOUT", "SIZE", "MODIFIED"],
            ResourceKind::Asg => &["NAME", "DESIRED", "MIN", "MAX", "INSTANCES", "AZS", "AGE"],
            ResourceKind::Rds => &[
                "NAME", "ENGINE", "VERSION", "CLASS", "STATUS", "STORAGE", "MULTI-AZ", "AGE",
            ],
            ResourceKind::Dynamo => &["NAME", "STATUS", "ITEMS", "SIZE", "BILLING", "AGE"],
            ResourceKind::Elb => &["NAME", "TYPE", "SCHEME", "STATE", "VPC-ID", "AZS", "AGE"],
            ResourceKind::Sqs => &["NAME", "MESSAGES", "IN-FLIGHT", "AGE"],
            ResourceKind::Sns => &["NAME", "SUBS", "ARN"],
            ResourceKind::Ecs => &[
                "NAME",
                "STATUS",
                "SERVICES",
                "RUNNING",
                "PENDING",
                "INSTANCES",
            ],
            ResourceKind::Eks => &["NAME", "STATUS", "VERSION", "PLATFORM", "AGE"],
            ResourceKind::Cfn => &["NAME", "STATUS", "AGE", "UPDATED"],
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

/// One describe-dashboard panel: an AWS-official section title (Details,
/// Networking, Security, Storage, Tags, …) and its key/value rows.
pub type DetailSection = (String, Vec<(String, String)>);

/// One row of a generic resource table. cells aligns with kind.columns();
/// warn/crit mark cells for orange/red highlighting (orphaned volumes,
/// unassociated EIPs, low FREE-IPS — the "billed waste" signals).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceRow {
    pub id: String,
    pub cells: Vec<String>,
    pub warn_cells: Vec<usize>,
    pub crit_cells: Vec<usize>,
    /// The grouped full record for the describe dashboard (includes tags).
    pub detail: Vec<DetailSection>,
}

/// The outcome of a resource list call.
#[derive(Debug, Default)]
pub struct ResourceList {
    pub kind: ResourceKind,
    pub rows: Vec<ResourceRow>,
    pub warnings: Vec<Warning>,
}

// ---- helpers shared by the per-service fetchers ----

pub(crate) fn aws_err(e: &(impl std::error::Error + 'static)) -> String {
    format!("{}", aws_sdk_ec2::error::DisplayErrorContext(e))
}

/// Compact relative age, "-" when the API exposes no creation time.
pub(crate) fn age(t: Option<DateTime<Utc>>) -> String {
    let Some(t) = t else {
        return "-".to_string();
    };
    let s = crate::tui::age_label(Some(t));
    if s.is_empty() { "-".to_string() } else { s }
}

/// SDK timestamp → chrono (the smithy DateTime is shared by every service
/// crate; the ec2 re-export names the same type).
pub(crate) fn aws_time(t: Option<&aws_sdk_ec2::primitives::DateTime>) -> Option<DateTime<Utc>> {
    t.and_then(|t| DateTime::from_timestamp(t.secs(), t.subsec_nanos()))
}

pub(crate) fn dash(s: &str) -> String {
    if s.is_empty() {
        "-".to_string()
    } else {
        s.to_string()
    }
}

/// Builds one detail section, dropping empty values ("-" stays).
pub(crate) fn sec(title: &str, rows: Vec<(&str, String)>) -> DetailSection {
    (
        title.to_string(),
        rows.into_iter()
            .map(|(k, v)| (k.to_string(), if v.is_empty() { "-".into() } else { v }))
            .collect(),
    )
}

/// Compact human bytes (the console style: 12 KB / 4.2 MB / 1.1 GB).
pub(crate) fn fmt_bytes(b: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else if v >= 10.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Fetches one resource kind from AWS. Failures come back as Warnings with
/// however many rows were listed before the error (usually none). The EC2
/// family reuses the long-lived client; the other services build theirs from
/// the SdkConfig per call (construction is cheap, the config is shared).
pub(crate) async fn list_aws(
    ec2: &aws_sdk_ec2::Client,
    cfg: &aws_config::SdkConfig,
    kind: ResourceKind,
) -> ResourceList {
    let mut res = ResourceList {
        kind,
        ..Default::default()
    };
    match kind {
        ResourceKind::Instances => {} // handled by inventory::list
        ResourceKind::Volumes => ec2::volumes(ec2, &mut res).await,
        ResourceKind::Snapshots => ec2::snapshots(ec2, &mut res).await,
        ResourceKind::SecurityGroups => ec2::security_groups(ec2, &mut res).await,
        ResourceKind::Vpcs => ec2::vpcs(ec2, &mut res).await,
        ResourceKind::Subnets => ec2::subnets(ec2, &mut res).await,
        ResourceKind::Enis => ec2::enis(ec2, &mut res).await,
        ResourceKind::Eips => ec2::eips(ec2, &mut res).await,
        ResourceKind::Amis => ec2::amis(ec2, &mut res).await,
        ResourceKind::S3 => services::buckets(cfg, &mut res).await,
        ResourceKind::Lambda => services::functions(cfg, &mut res).await,
        ResourceKind::Asg => services::asgs(cfg, &mut res).await,
        ResourceKind::Rds => services::rds_instances(cfg, &mut res).await,
        ResourceKind::Dynamo => services::dynamo_tables(cfg, &mut res).await,
        ResourceKind::Elb => services::load_balancers(cfg, &mut res).await,
        ResourceKind::Sqs => services::queues(cfg, &mut res).await,
        ResourceKind::Sns => services::topics(cfg, &mut res).await,
        ResourceKind::Ecs => services::ecs_clusters(cfg, &mut res).await,
        ResourceKind::Eks => services::eks_clusters(cfg, &mut res).await,
        ResourceKind::Cfn => services::stacks(cfg, &mut res).await,
    }
    res.rows
        .sort_by(|a, b| (a.cells.first(), &a.id).cmp(&(b.cells.first(), &b.id)));
    res
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
    fn every_kind_has_a_category() {
        for k in std::iter::once(ResourceKind::Instances).chain(KINDS) {
            assert!(!k.category().is_empty(), "{k:?} needs a category");
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
                assert!(
                    r.detail
                        .iter()
                        .all(|(t, rows)| !t.is_empty() && !rows.is_empty()),
                    "{k:?} row {} has an empty detail section",
                    r.id
                );
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
