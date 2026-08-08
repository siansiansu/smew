//! Fetchers for the non-EC2 resource views: S3, Lambda, Auto Scaling, RDS,
//! DynamoDB, ELBv2, SQS, SNS, ECS, EKS and CloudFormation.
//!
//! Each view is one paginated list call; kinds whose list API returns only
//! names/ARNs (DynamoDB, SQS, SNS, EKS) enrich rows with per-item describe
//! calls, 8 at a time (same politeness as the CloudWatch fetch).

use aws_config::SdkConfig;
use chrono::{DateTime, Utc};

use crate::inventory::Warning;

use super::{ResourceList, ResourceRow, age, aws_err, aws_time, dash, fmt_bytes, sec};

/// How many per-item describe calls run at once.
const DESCRIBE_CONCURRENCY: usize = 8;

/// Lambda's LastModified is an ISO-8601 string with a numeric offset
/// ("2024-01-01T00:00:00.000+0000") — not quite RFC 3339.
fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z").ok())
        .map(|d| d.with_timezone(&Utc))
}

pub(super) async fn buckets(cfg: &SdkConfig, res: &mut ResourceList) {
    let s3 = aws_sdk_s3::Client::new(cfg);
    let mut pages = s3.list_buckets().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "s3:ListBuckets",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for b in page.buckets() {
            let name = b.name().unwrap_or_default().to_string();
            let created = aws_time(b.creation_date());
            let region = b.bucket_region().unwrap_or_default().to_string();
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![name.clone(), dash(&region), age(created)],
                ..Default::default()
            };
            row.detail = vec![sec(
                "Details",
                vec![
                    ("Name", name),
                    ("Region", region),
                    (
                        "Created",
                        created.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    ),
                ],
            )];
            res.rows.push(row);
        }
    }
}

pub(super) async fn functions(cfg: &SdkConfig, res: &mut ResourceList) {
    let lambda = aws_sdk_lambda::Client::new(cfg);
    let mut pages = lambda.list_functions().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "lambda:ListFunctions",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for f in page.functions() {
            let name = f.function_name().unwrap_or_default().to_string();
            let runtime = f.runtime().map(|r| r.as_str()).unwrap_or("-");
            let modified = f.last_modified().and_then(parse_iso);
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![
                    name.clone(),
                    runtime.into(),
                    f.memory_size().map(|m| format!("{m}")).unwrap_or_default(),
                    f.timeout().map(|t| format!("{t}s")).unwrap_or_default(),
                    fmt_bytes(f.code_size()),
                    age(modified),
                ],
                ..Default::default()
            };
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        ("Arn", f.function_arn().unwrap_or_default().into()),
                        ("Description", f.description().unwrap_or_default().into()),
                        ("LastModified", f.last_modified().unwrap_or_default().into()),
                    ],
                ),
                sec(
                    "Configuration",
                    vec![
                        ("Runtime", runtime.into()),
                        ("Handler", f.handler().unwrap_or_default().into()),
                        (
                            "Memory (MB)",
                            f.memory_size().map(|m| m.to_string()).unwrap_or_default(),
                        ),
                        (
                            "Timeout (s)",
                            f.timeout().map(|t| t.to_string()).unwrap_or_default(),
                        ),
                        ("CodeSize", fmt_bytes(f.code_size())),
                        (
                            "Architectures",
                            f.architectures()
                                .iter()
                                .map(|a| a.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn asgs(cfg: &SdkConfig, res: &mut ResourceList) {
    let asg = aws_sdk_autoscaling::Client::new(cfg);
    let mut pages = asg.describe_auto_scaling_groups().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "autoscaling:DescribeAutoScalingGroups",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for g in page.auto_scaling_groups() {
            let name = g.auto_scaling_group_name().unwrap_or_default().to_string();
            let desired = g.desired_capacity().unwrap_or_default();
            let live = g.instances().len() as i32;
            let healthy = g
                .instances()
                .iter()
                .filter(|i| i.health_status() == Some("Healthy"))
                .count();
            let azs: Vec<&str> = g.availability_zones().iter().map(String::as_str).collect();
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![
                    name.clone(),
                    format!("{desired}"),
                    g.min_size().map(|m| format!("{m}")).unwrap_or_default(),
                    g.max_size().map(|m| format!("{m}")).unwrap_or_default(),
                    format!("{live}"),
                    format!("{}", azs.len()),
                    age(aws_time(g.created_time())),
                ],
                ..Default::default()
            };
            if live < desired {
                row.warn_cells.push(4); // under capacity
            }
            let launch = g
                .launch_template()
                .and_then(|t| t.launch_template_name())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    g.launch_configuration_name()
                        .unwrap_or_default()
                        .to_string()
                });
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        ("LaunchTemplate", launch),
                        (
                            "Created",
                            aws_time(g.created_time())
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Capacity",
                    vec![
                        ("Desired", desired.to_string()),
                        (
                            "Min / Max",
                            format!(
                                "{} / {}",
                                g.min_size().unwrap_or_default(),
                                g.max_size().unwrap_or_default()
                            ),
                        ),
                        ("Instances", live.to_string()),
                        ("Healthy", healthy.to_string()),
                    ],
                ),
                sec("Networking", vec![("AZs", azs.join(", "))]),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn rds_instances(cfg: &SdkConfig, res: &mut ResourceList) {
    let rds = aws_sdk_rds::Client::new(cfg);
    let mut pages = rds.describe_db_instances().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "rds:DescribeDBInstances",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for d in page.db_instances() {
            let name = d.db_instance_identifier().unwrap_or_default().to_string();
            let status = d.db_instance_status().unwrap_or_default().to_string();
            let multi_az = d.multi_az().unwrap_or_default();
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![
                    name.clone(),
                    dash(d.engine().unwrap_or_default()),
                    dash(d.engine_version().unwrap_or_default()),
                    dash(d.db_instance_class().unwrap_or_default()),
                    dash(&status),
                    d.allocated_storage()
                        .map(|s| format!("{s}"))
                        .unwrap_or_default(),
                    if multi_az { "yes" } else { "-" }.into(),
                    age(aws_time(d.instance_create_time())),
                ],
                ..Default::default()
            };
            match status.as_str() {
                "available" | "backing-up" => {}
                "failed" | "storage-full" | "incompatible-parameters" => row.crit_cells.push(4),
                _ => row.warn_cells.push(4), // stopped / modifying / rebooting …
            }
            let endpoint = d
                .endpoint()
                .map(|e| {
                    format!(
                        "{}:{}",
                        e.address().unwrap_or_default(),
                        e.port().unwrap_or_default()
                    )
                })
                .unwrap_or_default();
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Identifier", name),
                        ("Engine", d.engine().unwrap_or_default().into()),
                        ("Version", d.engine_version().unwrap_or_default().into()),
                        ("Class", d.db_instance_class().unwrap_or_default().into()),
                        ("Status", status),
                        (
                            "Created",
                            aws_time(d.instance_create_time())
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Storage",
                    vec![
                        (
                            "Allocated (GiB)",
                            d.allocated_storage()
                                .map(|s| s.to_string())
                                .unwrap_or_default(),
                        ),
                        ("Type", d.storage_type().unwrap_or_default().into()),
                        (
                            "Encrypted",
                            d.storage_encrypted().unwrap_or_default().to_string(),
                        ),
                    ],
                ),
                sec(
                    "Networking",
                    vec![
                        ("Endpoint", endpoint),
                        ("AZ", d.availability_zone().unwrap_or_default().into()),
                        ("MultiAZ", multi_az.to_string()),
                        (
                            "VpcId",
                            d.db_subnet_group()
                                .and_then(|g| g.vpc_id())
                                .unwrap_or_default()
                                .into(),
                        ),
                        (
                            "PubliclyAccessible",
                            d.publicly_accessible().unwrap_or_default().to_string(),
                        ),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn dynamo_tables(cfg: &SdkConfig, res: &mut ResourceList) {
    let ddb = aws_sdk_dynamodb::Client::new(cfg);
    let mut names: Vec<String> = Vec::new();
    let mut pages = ddb.list_tables().into_paginator().send();
    while let Some(page) = pages.next().await {
        match page {
            Ok(p) => names.extend(p.table_names().iter().cloned()),
            Err(e) => {
                res.warnings.push(Warning {
                    op: "dynamodb:ListTables",
                    err: aws_err(&e),
                });
                break;
            }
        }
    }
    for chunk in names.chunks(DESCRIBE_CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for name in chunk.iter().cloned() {
            let ddb = ddb.clone();
            set.spawn(async move {
                let out = ddb.describe_table().table_name(&name).send().await;
                (name, out)
            });
        }
        while let Some(Ok((name, out))) = set.join_next().await {
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![
                    name.clone(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                ],
                detail: vec![sec("Details", vec![("Name", name.clone())])],
                ..Default::default()
            };
            match out {
                Err(e) => {
                    if !res
                        .warnings
                        .iter()
                        .any(|w| w.op == "dynamodb:DescribeTable")
                    {
                        res.warnings.push(Warning {
                            op: "dynamodb:DescribeTable",
                            err: aws_err(&e),
                        });
                    }
                }
                Ok(out) => {
                    if let Some(t) = out.table() {
                        let status = t.table_status().map(|s| s.as_str()).unwrap_or("-");
                        // absent BillingModeSummary means provisioned (the
                        // API only reports it for on-demand tables)
                        let billing = t
                            .billing_mode_summary()
                            .and_then(|b| b.billing_mode())
                            .map(|b| b.as_str())
                            .unwrap_or("PROVISIONED");
                        let created = aws_time(t.creation_date_time());
                        row.cells = vec![
                            name.clone(),
                            status.into(),
                            t.item_count().map(|i| format!("{i}")).unwrap_or_default(),
                            fmt_bytes(t.table_size_bytes().unwrap_or_default()),
                            billing.into(),
                            age(created),
                        ];
                        if status != "ACTIVE" {
                            row.warn_cells.push(1);
                        }
                        row.detail = vec![
                            sec(
                                "Details",
                                vec![
                                    ("Name", name),
                                    ("Status", status.into()),
                                    ("Arn", t.table_arn().unwrap_or_default().into()),
                                    (
                                        "Created",
                                        created.map(|t| t.to_rfc3339()).unwrap_or_default(),
                                    ),
                                ],
                            ),
                            sec(
                                "Capacity",
                                vec![
                                    (
                                        "Items",
                                        t.item_count().map(|i| i.to_string()).unwrap_or_default(),
                                    ),
                                    ("Size", fmt_bytes(t.table_size_bytes().unwrap_or_default())),
                                    ("Billing", billing.into()),
                                    (
                                        "RCU / WCU",
                                        t.provisioned_throughput()
                                            .map(|p| {
                                                format!(
                                                    "{} / {}",
                                                    p.read_capacity_units().unwrap_or_default(),
                                                    p.write_capacity_units().unwrap_or_default()
                                                )
                                            })
                                            .unwrap_or_default(),
                                    ),
                                ],
                            ),
                        ];
                    }
                }
            }
            res.rows.push(row);
        }
    }
}

pub(super) async fn load_balancers(cfg: &SdkConfig, res: &mut ResourceList) {
    let elb = aws_sdk_elasticloadbalancingv2::Client::new(cfg);
    let mut pages = elb.describe_load_balancers().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "elasticloadbalancing:DescribeLoadBalancers",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for lb in page.load_balancers() {
            let name = lb.load_balancer_name().unwrap_or_default().to_string();
            let state = lb
                .state()
                .and_then(|s| s.code())
                .map(|c| c.as_str())
                .unwrap_or("-");
            let azs: Vec<&str> = lb
                .availability_zones()
                .iter()
                .filter_map(|z| z.zone_name())
                .collect();
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![
                    name.clone(),
                    lb.r#type().map(|t| t.as_str()).unwrap_or("-").into(),
                    lb.scheme().map(|s| s.as_str()).unwrap_or("-").into(),
                    state.into(),
                    dash(lb.vpc_id().unwrap_or_default()),
                    format!("{}", azs.len()),
                    age(aws_time(lb.created_time())),
                ],
                ..Default::default()
            };
            match state {
                "active" => {}
                "failed" => row.crit_cells.push(3),
                _ => row.warn_cells.push(3), // provisioning / active_impaired
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        (
                            "Type",
                            lb.r#type().map(|t| t.as_str()).unwrap_or("-").into(),
                        ),
                        (
                            "Scheme",
                            lb.scheme().map(|s| s.as_str()).unwrap_or("-").into(),
                        ),
                        ("State", state.into()),
                        (
                            "Created",
                            aws_time(lb.created_time())
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Networking",
                    vec![
                        ("DNS", lb.dns_name().unwrap_or_default().into()),
                        ("VpcId", lb.vpc_id().unwrap_or_default().into()),
                        ("AZs", azs.join(", ")),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn queues(cfg: &SdkConfig, res: &mut ResourceList) {
    use aws_sdk_sqs::types::QueueAttributeName;

    let sqs = aws_sdk_sqs::Client::new(cfg);
    let mut urls: Vec<String> = Vec::new();
    let mut pages = sqs.list_queues().into_paginator().send();
    while let Some(page) = pages.next().await {
        match page {
            Ok(p) => urls.extend(p.queue_urls().iter().cloned()),
            Err(e) => {
                res.warnings.push(Warning {
                    op: "sqs:ListQueues",
                    err: aws_err(&e),
                });
                break;
            }
        }
    }
    for chunk in urls.chunks(DESCRIBE_CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for url in chunk.iter().cloned() {
            let sqs = sqs.clone();
            set.spawn(async move {
                let out = sqs
                    .get_queue_attributes()
                    .queue_url(&url)
                    .attribute_names(QueueAttributeName::All)
                    .send()
                    .await;
                (url, out)
            });
        }
        while let Some(Ok((url, out))) = set.join_next().await {
            let name = url.rsplit('/').next().unwrap_or(&url).to_string();
            let attrs = match out {
                Ok(o) => o.attributes.unwrap_or_default(),
                Err(e) => {
                    if !res
                        .warnings
                        .iter()
                        .any(|w| w.op == "sqs:GetQueueAttributes")
                    {
                        res.warnings.push(Warning {
                            op: "sqs:GetQueueAttributes",
                            err: aws_err(&e),
                        });
                    }
                    Default::default()
                }
            };
            let get = |k: QueueAttributeName| attrs.get(&k).cloned().unwrap_or_default();
            let visible = get(QueueAttributeName::ApproximateNumberOfMessages);
            let inflight = get(QueueAttributeName::ApproximateNumberOfMessagesNotVisible);
            let created = get(QueueAttributeName::CreatedTimestamp)
                .parse::<i64>()
                .ok()
                .and_then(|s| DateTime::from_timestamp(s, 0));
            let mut row = ResourceRow {
                id: url.clone(),
                cells: vec![name.clone(), dash(&visible), dash(&inflight), age(created)],
                ..Default::default()
            };
            // a filling dead-letter queue is the classic silent failure
            if name.contains("dlq") && visible.parse::<i64>().unwrap_or(0) > 0 {
                row.warn_cells.push(1);
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        ("Url", url),
                        ("Arn", get(QueueAttributeName::QueueArn)),
                        (
                            "Created",
                            created.map(|t| t.to_rfc3339()).unwrap_or_default(),
                        ),
                    ],
                ),
                sec(
                    "Messages",
                    vec![
                        ("Visible", visible),
                        ("In flight", inflight),
                        (
                            "Delayed",
                            get(QueueAttributeName::ApproximateNumberOfMessagesDelayed),
                        ),
                    ],
                ),
                sec(
                    "Configuration",
                    vec![
                        (
                            "VisibilityTimeout (s)",
                            get(QueueAttributeName::VisibilityTimeout),
                        ),
                        (
                            "Retention (s)",
                            get(QueueAttributeName::MessageRetentionPeriod),
                        ),
                        ("Fifo", get(QueueAttributeName::FifoQueue)),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn topics(cfg: &SdkConfig, res: &mut ResourceList) {
    let sns = aws_sdk_sns::Client::new(cfg);
    let mut arns: Vec<String> = Vec::new();
    let mut pages = sns.list_topics().into_paginator().send();
    while let Some(page) = pages.next().await {
        match page {
            Ok(p) => arns.extend(
                p.topics()
                    .iter()
                    .filter_map(|t| t.topic_arn())
                    .map(String::from),
            ),
            Err(e) => {
                res.warnings.push(Warning {
                    op: "sns:ListTopics",
                    err: aws_err(&e),
                });
                break;
            }
        }
    }
    for chunk in arns.chunks(DESCRIBE_CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for arn in chunk.iter().cloned() {
            let sns = sns.clone();
            set.spawn(async move {
                let out = sns.get_topic_attributes().topic_arn(&arn).send().await;
                (arn, out)
            });
        }
        while let Some(Ok((arn, out))) = set.join_next().await {
            let name = arn.rsplit(':').next().unwrap_or(&arn).to_string();
            let attrs = match out {
                Ok(o) => o.attributes.unwrap_or_default(),
                Err(e) => {
                    if !res
                        .warnings
                        .iter()
                        .any(|w| w.op == "sns:GetTopicAttributes")
                    {
                        res.warnings.push(Warning {
                            op: "sns:GetTopicAttributes",
                            err: aws_err(&e),
                        });
                    }
                    Default::default()
                }
            };
            let get = |k: &str| attrs.get(k).cloned().unwrap_or_default();
            let subs = get("SubscriptionsConfirmed");
            let mut row = ResourceRow {
                id: arn.clone(),
                cells: vec![name.clone(), dash(&subs), arn.clone()],
                ..Default::default()
            };
            if subs.parse::<i64>().unwrap_or(-1) == 0 {
                row.warn_cells.push(1); // nobody listens to this topic
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        ("Arn", arn),
                        ("DisplayName", get("DisplayName")),
                        ("Fifo", get("FifoTopic")),
                    ],
                ),
                sec(
                    "Subscriptions",
                    vec![
                        ("Confirmed", subs),
                        ("Pending", get("SubscriptionsPending")),
                        ("Deleted", get("SubscriptionsDeleted")),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn ecs_clusters(cfg: &SdkConfig, res: &mut ResourceList) {
    let ecs = aws_sdk_ecs::Client::new(cfg);
    let mut arns: Vec<String> = Vec::new();
    let mut pages = ecs.list_clusters().into_paginator().send();
    while let Some(page) = pages.next().await {
        match page {
            Ok(p) => arns.extend(p.cluster_arns().iter().cloned()),
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ecs:ListClusters",
                    err: aws_err(&e),
                });
                break;
            }
        }
    }
    // DescribeClusters takes up to 100 clusters per call.
    for chunk in arns.chunks(100) {
        let out = match ecs
            .describe_clusters()
            .set_clusters(Some(chunk.to_vec()))
            .send()
            .await
        {
            Ok(o) => o,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "ecs:DescribeClusters",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for c in out.clusters() {
            let name = c.cluster_name().unwrap_or_default().to_string();
            let status = c.status().unwrap_or_default().to_string();
            let pending = c.pending_tasks_count();
            let mut row = ResourceRow {
                id: c.cluster_arn().unwrap_or_default().to_string(),
                cells: vec![
                    name.clone(),
                    dash(&status),
                    format!("{}", c.active_services_count()),
                    format!("{}", c.running_tasks_count()),
                    format!("{pending}"),
                    format!("{}", c.registered_container_instances_count()),
                ],
                ..Default::default()
            };
            if status != "ACTIVE" {
                row.warn_cells.push(1);
            }
            if pending > 0 {
                row.warn_cells.push(4); // tasks stuck waiting for capacity
            }
            row.detail = vec![
                sec(
                    "Details",
                    vec![
                        ("Name", name),
                        ("Arn", c.cluster_arn().unwrap_or_default().into()),
                        ("Status", status),
                    ],
                ),
                sec(
                    "Tasks",
                    vec![
                        ("Services", c.active_services_count().to_string()),
                        ("Running", c.running_tasks_count().to_string()),
                        ("Pending", pending.to_string()),
                        (
                            "Container instances",
                            c.registered_container_instances_count().to_string(),
                        ),
                    ],
                ),
            ];
            res.rows.push(row);
        }
    }
}

pub(super) async fn eks_clusters(cfg: &SdkConfig, res: &mut ResourceList) {
    let eks = aws_sdk_eks::Client::new(cfg);
    let mut names: Vec<String> = Vec::new();
    let mut pages = eks.list_clusters().into_paginator().send();
    while let Some(page) = pages.next().await {
        match page {
            Ok(p) => names.extend(p.clusters().iter().cloned()),
            Err(e) => {
                res.warnings.push(Warning {
                    op: "eks:ListClusters",
                    err: aws_err(&e),
                });
                break;
            }
        }
    }
    for chunk in names.chunks(DESCRIBE_CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for name in chunk.iter().cloned() {
            let eks = eks.clone();
            set.spawn(async move {
                let out = eks.describe_cluster().name(&name).send().await;
                (name, out)
            });
        }
        while let Some(Ok((name, out))) = set.join_next().await {
            let mut row = ResourceRow {
                id: name.clone(),
                cells: vec![name.clone(), "-".into(), "-".into(), "-".into(), "-".into()],
                detail: vec![sec("Details", vec![("Name", name.clone())])],
                ..Default::default()
            };
            match out {
                Err(e) => {
                    if !res.warnings.iter().any(|w| w.op == "eks:DescribeCluster") {
                        res.warnings.push(Warning {
                            op: "eks:DescribeCluster",
                            err: aws_err(&e),
                        });
                    }
                }
                Ok(out) => {
                    if let Some(c) = out.cluster() {
                        let status = c.status().map(|s| s.as_str()).unwrap_or("-");
                        let created = aws_time(c.created_at());
                        row.cells = vec![
                            name.clone(),
                            status.into(),
                            dash(c.version().unwrap_or_default()),
                            dash(c.platform_version().unwrap_or_default()),
                            age(created),
                        ];
                        if status != "ACTIVE" {
                            row.warn_cells.push(1);
                        }
                        row.detail = vec![
                            sec(
                                "Details",
                                vec![
                                    ("Name", name),
                                    ("Status", status.into()),
                                    ("Version", c.version().unwrap_or_default().into()),
                                    ("Platform", c.platform_version().unwrap_or_default().into()),
                                    (
                                        "Created",
                                        created.map(|t| t.to_rfc3339()).unwrap_or_default(),
                                    ),
                                ],
                            ),
                            sec(
                                "Networking",
                                vec![
                                    ("Endpoint", c.endpoint().unwrap_or_default().into()),
                                    (
                                        "VpcId",
                                        c.resources_vpc_config()
                                            .and_then(|v| v.vpc_id())
                                            .unwrap_or_default()
                                            .into(),
                                    ),
                                ],
                            ),
                        ];
                    }
                }
            }
            res.rows.push(row);
        }
    }
}

pub(super) async fn stacks(cfg: &SdkConfig, res: &mut ResourceList) {
    let cfn = aws_sdk_cloudformation::Client::new(cfg);
    let mut pages = cfn.describe_stacks().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = match page {
            Ok(p) => p,
            Err(e) => {
                res.warnings.push(Warning {
                    op: "cloudformation:DescribeStacks",
                    err: aws_err(&e),
                });
                break;
            }
        };
        for s in page.stacks() {
            let name = s.stack_name().unwrap_or_default().to_string();
            let status = s.stack_status().map(|x| x.as_str()).unwrap_or("-");
            let updated = aws_time(s.last_updated_time());
            let mut row = ResourceRow {
                id: s.stack_id().unwrap_or(&name).to_string(),
                cells: vec![
                    name.clone(),
                    status.into(),
                    age(aws_time(s.creation_time())),
                    if updated.is_some() {
                        age(updated)
                    } else {
                        "-".into()
                    },
                ],
                ..Default::default()
            };
            if status.contains("FAILED") || status.contains("ROLLBACK") {
                row.crit_cells.push(1);
            } else if status.contains("IN_PROGRESS") {
                row.warn_cells.push(1);
            }
            row.detail = vec![sec(
                "Details",
                vec![
                    ("Name", name),
                    ("Status", status.into()),
                    (
                        "StatusReason",
                        s.stack_status_reason().unwrap_or_default().into(),
                    ),
                    ("Description", s.description().unwrap_or_default().into()),
                    (
                        "Created",
                        aws_time(s.creation_time())
                            .map(|t| t.to_rfc3339())
                            .unwrap_or_default(),
                    ),
                    (
                        "Updated",
                        updated.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    ),
                ],
            )];
            res.rows.push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lambda_last_modified() {
        // rfc3339 and Lambda's numeric-offset variants both parse
        assert!(parse_iso("2024-01-01T00:00:00.000+0000").is_some());
        assert!(parse_iso("2024-01-01T00:00:00+00:00").is_some());
        assert!(parse_iso("bogus").is_none());
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(340), "340 B");
        assert_eq!(fmt_bytes(12 * 1024), "12 KB");
        assert_eq!(fmt_bytes(4_500_000), "4.3 MB");
        assert_eq!(fmt_bytes(48 * 1024 * 1024), "48 MB");
    }
}
