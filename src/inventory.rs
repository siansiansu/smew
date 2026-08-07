//! Lists EC2 instances enriched with SSM reachability.
//!
//! It merges two data sources per account/region:
//!   - ec2:DescribeInstances       — VPC / subnet / SG / tags / IPs / type / AZ
//!   - ssm:DescribeInstanceInformation — agent online status (connectable?)
//!
//! Every API call degrades gracefully: a missing permission yields a Warning
//! and reduced detail, never a hard failure.

use std::collections::{BTreeMap, HashMap};

use aws_config::SdkConfig;
use aws_sdk_ec2::error::DisplayErrorContext;
use aws_sdk_ssm::types::PingStatus;
use chrono::{DateTime, Utc};

/// Renders an SDK error with its full cause chain (DisplayErrorContext).
fn aws_err(e: &(impl std::error::Error + 'static)) -> String {
    format!("{}", DisplayErrorContext(e))
}

/// A security group id + resolved name.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityGroupRef {
    pub id: String,
    pub name: String,
}

/// The SSM agent's reachability for an instance.
#[derive(Debug, Clone, PartialEq)]
pub struct SsmStatus {
    pub online: bool,
    pub agent_version: String,
    pub ping_status: String,
}

/// The merged EC2 + SSM view of a managed host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Instance {
    pub instance_id: String,
    pub name: String,
    pub state: String,
    pub instance_type: String,
    pub platform: String,
    pub az: String,
    pub vpc_id: String,
    pub subnet_id: String,
    pub security_groups: Vec<SecurityGroupRef>,
    pub private_ip: String,
    pub public_ip: String,
    pub launch_time: Option<DateTime<Utc>>,
    pub tags: BTreeMap<String, String>,
    pub ssm: Option<SsmStatus>,
}

impl Instance {
    /// Whether the instance is reachable via SSM right now.
    pub fn is_connectable(&self) -> bool {
        self.ssm.as_ref().is_some_and(|s| s.online)
    }
}

/// A non-fatal API/permission error surfaced to the UI.
#[derive(Debug, Clone)]
pub struct Warning {
    pub op: &'static str,
    pub err: String,
}

/// The outcome of a list call.
#[derive(Debug, Default)]
pub struct ListResult {
    pub instances: Vec<Instance>,
    pub warnings: Vec<Warning>,
}

/// Queries EC2 and SSM — or, in developer mode, serves a canned in-memory
/// dataset. Cheap to clone (SDK clients are Arc-backed).
#[derive(Clone)]
pub struct Inventory {
    backend: Backend,
}

#[derive(Clone)]
enum Backend {
    Aws {
        ec2: aws_sdk_ec2::Client,
        ssm: aws_sdk_ssm::Client,
    },
    /// Developer mode (--dev): no AWS calls; list() serves mock_instances().
    Mock,
}

impl Inventory {
    /// Builds an Inventory from a loaded SdkConfig.
    pub fn new(cfg: &SdkConfig) -> Self {
        Self {
            backend: Backend::Aws {
                ec2: aws_sdk_ec2::Client::new(cfg),
                ssm: aws_sdk_ssm::Client::new(cfg),
            },
        }
    }

    /// A no-AWS inventory for developer mode: list() returns a fixed varied
    /// dataset, mutations pretend to succeed.
    pub fn mock() -> Self {
        Self {
            backend: Backend::Mock,
        }
    }

    /// Requests an EC2 reboot of the instance (ec2:RebootInstances).
    pub async fn reboot(&self, instance_id: &str) -> Result<(), String> {
        let Backend::Aws { ec2, .. } = &self.backend else {
            return Ok(()); // dev mode: pretend success
        };
        ec2.reboot_instances()
            .instance_ids(instance_id)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| aws_err(&e))
    }

    /// Reads the published latest version from an SSM Parameter Store
    /// parameter (used for the update check). Returns "" if unset/empty.
    pub async fn latest_version(&self, param: &str) -> Result<String, String> {
        let Backend::Aws { ssm, .. } = &self.backend else {
            return Ok(String::new()); // dev mode: no update check
        };
        let out = ssm
            .get_parameter()
            .name(param)
            .send()
            .await
            .map_err(|e| aws_err(&e))?;
        Ok(out
            .parameter()
            .and_then(|p| p.value())
            .unwrap_or_default()
            .trim()
            .to_string())
    }

    /// Returns all instances for the configured account/region, merging EC2
    /// detail with SSM reachability. Partial failures are reported as Warnings.
    pub async fn list(&self) -> ListResult {
        let mut res = match &self.backend {
            Backend::Aws { ec2, ssm } => list_aws(ec2, ssm).await,
            Backend::Mock => ListResult {
                instances: mock_instances(),
                warnings: Vec::new(),
            },
        };
        res.instances
            .sort_by(|a, b| (&a.name, &a.instance_id).cmp(&(&b.name, &b.instance_id)));
        res
    }
}

/// The AWS-backed listing (the mock backend skips all of this).
async fn list_aws(ec2: &aws_sdk_ec2::Client, ssm: &aws_sdk_ssm::Client) -> ListResult {
    let mut res = ListResult::default();
    let mut by_id: HashMap<String, Instance> = HashMap::new();

    // EC2 detail — primary listing.
    let mut pages = ec2.describe_instances().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ec2:DescribeInstances",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for r in page.reservations() {
            for ec2_inst in r.instances() {
                let inst = Instance::from(ec2_inst);
                by_id.insert(inst.instance_id.clone(), inst);
            }
        }
    }

    // SSM reachability — merged onto EC2 records.
    let mut pages = ssm.describe_instance_information().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ssm:DescribeInstanceInformation",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for info in page.instance_information_list() {
            let id = info.instance_id().unwrap_or_default().to_string();
            let st = SsmStatus {
                online: info.ping_status() == Some(&PingStatus::Online),
                agent_version: info.agent_version().unwrap_or_default().to_string(),
                ping_status: info
                    .ping_status()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default(),
            };
            if let Some(inst) = by_id.get_mut(&id) {
                inst.ssm = Some(st);
                continue;
            }
            // SSM knows about it but EC2 describe didn't return it
            // (e.g. ec2:DescribeInstances denied). Keep a minimal record.
            let name = info.name().unwrap_or_default();
            by_id.insert(
                id.clone(),
                Instance {
                    name: if name.is_empty() {
                        id.clone()
                    } else {
                        name.to_string()
                    },
                    instance_id: id,
                    platform: info
                        .platform_type()
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_default(),
                    ssm: Some(st),
                    ..Default::default()
                },
            );
        }
    }

    res.instances = by_id.into_values().collect();
    res
}

/// The developer-mode dataset: a spread of states, types, AZs, ages and SSM
/// reachability so every list/detail/session code path has something to show.
fn mock_instances() -> Vec<Instance> {
    let now = chrono::Utc::now();
    let online = || {
        Some(SsmStatus {
            online: true,
            agent_version: "3.3.987.0".to_string(),
            ping_status: "Online".to_string(),
        })
    };
    /// The fields every mock host shares (identity, tags, network placement);
    /// the per-host struct literals below override the rest.
    fn base(name: &str, id: &str, az: &str, tag_pairs: &[(&str, &str)]) -> Instance {
        let mut tags: BTreeMap<String, String> = tag_pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if !name.is_empty() {
            tags.insert("Name".to_string(), name.to_string());
        }
        Instance {
            instance_id: id.to_string(),
            name: if name.is_empty() { id } else { name }.to_string(),
            state: "running".to_string(),
            platform: "Linux/UNIX".to_string(),
            az: az.to_string(),
            vpc_id: "vpc-0dev00000000dev0".to_string(),
            subnet_id: format!(
                "subnet-0dev0000000000{}",
                az.as_bytes()[az.len() - 1] as char
            ),
            security_groups: vec![SecurityGroupRef {
                id: "sg-0dev00000000dev0".to_string(),
                name: "dev-default".to_string(),
            }],
            tags,
            ..Default::default()
        }
    }

    vec![
        Instance {
            instance_type: "t3.small".to_string(),
            private_ip: "10.0.1.11".to_string(),
            launch_time: Some(now - chrono::Duration::days(30)),
            ssm: online(),
            ..base(
                "web-01",
                "i-0aaaaaaaaaaaaaa01",
                "ap-northeast-1a",
                &[("env", "dev"), ("role", "web")],
            )
        },
        Instance {
            instance_type: "t3.small".to_string(),
            private_ip: "10.0.1.12".to_string(),
            launch_time: Some(now - chrono::Duration::days(30)),
            ssm: online(),
            ..base(
                "web-02",
                "i-0aaaaaaaaaaaaaa02",
                "ap-northeast-1c",
                &[("env", "dev"), ("role", "web")],
            )
        },
        Instance {
            instance_type: "t3.medium".to_string(),
            private_ip: "10.0.2.21".to_string(),
            launch_time: Some(now - chrono::Duration::hours(5)),
            ssm: online(),
            ..base(
                "api-01",
                "i-0bbbbbbbbbbbbbb01",
                "ap-northeast-1a",
                &[("env", "dev"), ("role", "api")],
            )
        },
        Instance {
            instance_type: "t3.micro".to_string(),
            private_ip: "10.0.3.5".to_string(),
            launch_time: Some(now - chrono::Duration::days(90)),
            ssm: online(),
            ..base(
                "db-bastion",
                "i-0cccccccccccccc01",
                "ap-northeast-1a",
                &[("env", "dev"), ("role", "bastion")],
            )
        },
        // SSM agent registered but unreachable → listed, not connectable.
        Instance {
            instance_type: "c6i.large".to_string(),
            private_ip: "10.0.4.31".to_string(),
            launch_time: Some(now - chrono::Duration::minutes(45)),
            ssm: Some(SsmStatus {
                online: false,
                agent_version: "3.2.1550.0".to_string(),
                ping_status: "ConnectionLost".to_string(),
            }),
            ..base(
                "worker-01",
                "i-0dddddddddddddd01",
                "ap-northeast-1c",
                &[("env", "dev"), ("role", "worker")],
            )
        },
        // Stopped → no SSM record at all.
        Instance {
            state: "stopped".to_string(),
            instance_type: "t4g.small".to_string(),
            private_ip: "10.0.5.8".to_string(),
            launch_time: Some(now - chrono::Duration::days(45)),
            ssm: None,
            ..base(
                "cache-01",
                "i-0eeeeeeeeeeeeee01",
                "ap-northeast-1a",
                &[("env", "dev"), ("role", "cache")],
            )
        },
        // No Name tag → the id doubles as the display name.
        Instance {
            instance_type: "t2.micro".to_string(),
            private_ip: "10.0.9.99".to_string(),
            launch_time: Some(now - chrono::Duration::days(400)),
            ssm: online(),
            ..base("", "i-0ffffffffffffff01", "ap-northeast-1a", &[])
        },
        Instance {
            instance_type: "m5.large".to_string(),
            platform: "Windows".to_string(),
            private_ip: "10.0.6.14".to_string(),
            public_ip: "203.0.113.20".to_string(),
            launch_time: Some(now - chrono::Duration::days(7)),
            ssm: online(),
            ..base(
                "windows-01",
                "i-0abcdefabcdefab01",
                "ap-northeast-1c",
                &[("env", "dev"), ("role", "win")],
            )
        },
    ]
}

impl From<&aws_sdk_ec2::types::Instance> for Instance {
    fn from(ec2_inst: &aws_sdk_ec2::types::Instance) -> Instance {
        let mut inst = Instance {
            instance_id: ec2_inst.instance_id().unwrap_or_default().to_string(),
            instance_type: ec2_inst
                .instance_type()
                .map(|t| t.as_str().to_string())
                .unwrap_or_default(),
            platform: ec2_inst.platform_details().unwrap_or_default().to_string(),
            vpc_id: ec2_inst.vpc_id().unwrap_or_default().to_string(),
            subnet_id: ec2_inst.subnet_id().unwrap_or_default().to_string(),
            private_ip: ec2_inst
                .private_ip_address()
                .unwrap_or_default()
                .to_string(),
            public_ip: ec2_inst.public_ip_address().unwrap_or_default().to_string(),
            launch_time: ec2_inst
                .launch_time()
                .and_then(|t| DateTime::from_timestamp(t.secs(), t.subsec_nanos())),
            ..Default::default()
        };
        if let Some(state) = ec2_inst.state() {
            inst.state = state
                .name()
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
        }
        if let Some(p) = ec2_inst.placement() {
            inst.az = p.availability_zone().unwrap_or_default().to_string();
        }
        for t in ec2_inst.tags() {
            let (k, v) = (t.key().unwrap_or_default(), t.value().unwrap_or_default());
            inst.tags.insert(k.to_string(), v.to_string());
            if k == "Name" {
                inst.name = v.to_string();
            }
        }
        for sg in ec2_inst.security_groups() {
            inst.security_groups.push(SecurityGroupRef {
                id: sg.group_id().unwrap_or_default().to_string(),
                name: sg.group_name().unwrap_or_default().to_string(),
            });
        }
        if inst.name.is_empty() {
            inst.name = inst.instance_id.clone();
        }
        inst
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Developer mode must serve a useful offline dataset: sorted like the
    // AWS path, with both connectable and non-connectable hosts, and no-op
    // mutations, so every UI flow can be exercised without credentials.
    #[test]
    fn mock_backend_serves_offline_data() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let inv = Inventory::mock();
        let res = rt.block_on(inv.list());

        assert!(res.warnings.is_empty());
        assert!(res.instances.len() >= 6, "got {}", res.instances.len());
        let keys: Vec<_> = res
            .instances
            .iter()
            .map(|i| (i.name.clone(), i.instance_id.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "mock list must be sorted like the AWS path");

        assert!(res.instances.iter().any(|i| i.is_connectable()));
        assert!(res.instances.iter().any(|i| !i.is_connectable()));
        assert!(res.instances.iter().any(|i| i.state == "stopped"));
        assert!(
            res.instances.iter().any(|i| i.name == i.instance_id),
            "want an unnamed instance (name falls back to the id)"
        );

        assert_eq!(rt.block_on(inv.reboot("i-whatever")), Ok(()));
        assert_eq!(
            rt.block_on(inv.latest_version("/any/param")),
            Ok(String::new())
        );
    }
}
