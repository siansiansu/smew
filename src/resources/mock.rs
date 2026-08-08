//! Developer-mode fixtures for the resource views.

use super::{ResourceKind, ResourceRow};

/// Dev-mode rows, referentially consistent with inventory::mock_instances
/// (same vpc/subnet/sg/instance ids) so drill-down navigation works offline.
/// Each kind carries its edge cases: orphaned/errored volumes, a stuck and
/// a zero-progress snapshot, AWS-managed ENIs (NAT/endpoint/Lambda), idle
/// EIPs, subnets near IP exhaustion, and a failed AMI bake.
pub(crate) fn mock(kind: ResourceKind) -> Vec<ResourceRow> {
    let row = |id: &str, cells: &[&str]| ResourceRow {
        id: id.to_string(),
        cells: cells.iter().map(|s| s.to_string()).collect(),
        detail: vec![
            (
                "Details".to_string(),
                std::iter::once(("Id".to_string(), id.to_string()))
                    .chain(
                        kind.columns()
                            .iter()
                            .zip(cells.iter())
                            .map(|(k, v)| (k.to_string(), v.to_string())),
                    )
                    .collect(),
            ),
            (
                "Tags".to_string(),
                vec![("env".to_string(), "dev".to_string())],
            ),
        ],
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
