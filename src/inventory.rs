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
#[derive(Debug, Clone)]
pub struct SecurityGroupRef {
    pub id: String,
    pub name: String,
}

/// The SSM agent's reachability for an instance.
#[derive(Debug, Clone)]
pub struct SsmStatus {
    pub online: bool,
    pub agent_version: String,
    pub ping_status: String,
}

/// The merged EC2 + SSM view of a managed host.
#[derive(Debug, Clone, Default)]
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
#[derive(Debug)]
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

/// Queries EC2 and SSM. Cheap to clone (SDK clients are Arc-backed).
#[derive(Clone)]
pub struct Inventory {
    ec2: aws_sdk_ec2::Client,
    ssm: aws_sdk_ssm::Client,
}

impl Inventory {
    /// Builds an Inventory from a loaded SdkConfig.
    pub fn new(cfg: &SdkConfig) -> Self {
        Self {
            ec2: aws_sdk_ec2::Client::new(cfg),
            ssm: aws_sdk_ssm::Client::new(cfg),
        }
    }

    /// Requests an EC2 reboot of the instance (ec2:RebootInstances).
    pub async fn reboot(&self, instance_id: &str) -> Result<(), String> {
        self.ec2
            .reboot_instances()
            .instance_ids(instance_id)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| aws_err(&e))
    }

    /// Reads the published latest version from an SSM Parameter Store
    /// parameter (used for the update check). Returns "" if unset/empty.
    pub async fn latest_version(&self, param: &str) -> Result<String, String> {
        let out = self
            .ssm
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
        let mut res = ListResult::default();
        let mut by_id: HashMap<String, Instance> = HashMap::new();

        // EC2 detail — primary listing.
        let mut pages = self.ec2.describe_instances().into_paginator().send();
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
        let mut pages = self
            .ssm
            .describe_instance_information()
            .into_paginator()
            .send();
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
        res.instances
            .sort_by(|a, b| (&a.name, &a.instance_id).cmp(&(&b.name, &b.instance_id)));
        res
    }
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
