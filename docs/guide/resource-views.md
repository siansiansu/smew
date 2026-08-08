# Resource views

The main table shows one AWS resource kind at a time. Press `:` and type
an abbreviation to switch; `Tab` completes, `↑` recalls the last command,
and a unique prefix is accepted as-is. `:ec2` is home, `Esc` (or `q`)
returns to it.

## Every view

| AWS category | View | Aliases | Columns |
| --- | --- | --- | --- |
| Compute | `ec2` | `instances`, `i` | SSM reachability, name, id, state, type, %CPU/%MEM (opt-in), age, AZ, IP, VPC |
| Compute | `ami` | `images` | id, state, architecture, platform, public, age |
| Compute | `lambda` | `fn`, `functions` | runtime, memory, timeout, code size, last modified |
| Compute | `asg` | `autoscaling` | desired / min / max, live instances, AZs, age |
| Storage | `vol` | `ebs`, `volumes` | state, type, size, IOPS, attached instance, AZ, age |
| Storage | `snap` | `snapshots` | state, progress, size, source volume, encryption, age |
| Storage | `s3` | `buckets` | region, age |
| Database | `rds` | `db` | engine, version, class, status, storage, multi-AZ, age |
| Database | `ddb` | `dynamodb`, `tables` | status, item count, size, billing mode, age |
| Networking & Content Delivery | `vpc` | `vpcs` | CIDR, state, default, tenancy |
| Networking & Content Delivery | `subnet` | `sub`, `subnets` | VPC, CIDR, AZ, free IPs, public |
| Networking & Content Delivery | `eni` | `networkinterfaces` | status, private/public IP, type, attached to |
| Networking & Content Delivery | `eip` | `addresses` | public IP, allocation, association, private IP |
| Networking & Content Delivery | `elb` | `lb`, `alb`, `loadbalancers` | type, scheme, state, VPC, AZs, age |
| Security, Identity & Compliance | `sg` | `securitygroups` | VPC, ingress/egress rule counts, description |
| Application Integration | `sqs` | `queues` | visible messages, in flight, age |
| Application Integration | `sns` | `topics` | confirmed subscriptions, ARN |
| Containers | `ecs` | | status, services, running/pending tasks, container instances |
| Containers | `eks` | | status, version, platform, age |
| Management & Governance | `cfn` | `stacks`, `cloudformation` | status, age, last update |

Filtering (`/`), sorting, vim motions and the horizontal scroll work the
same in every view. All list calls are read-only describes; a missing
permission turns into a status-bar warning for that view, nothing more.

## Warning highlights

Cells turn orange for money leaks and red for broken states:

- unattached EBS volumes, volumes in `error`
- idle Elastic IPs (billed while unassociated)
- orphaned network interfaces
- subnets at ≤20% free IPs (red at ≤10%, relative to the CIDR size)
- dead-letter queues holding messages
- Auto Scaling groups running below their desired capacity
- SNS topics with zero subscribers
- CloudFormation stacks in `ROLLBACK`/`FAILED` states, `IN_PROGRESS` churn
- RDS instances in any status other than `available`, failed AMI bakes,
  non-active ECS/EKS clusters

## Describe dashboards

`Enter` (or `d`) opens the selected row as a dashboard: bordered panels
named after the console's own tabs — Details, Networking, Security,
Storage, Monitoring, Tags — packed into up to three columns so the whole
record fits one screen. Security groups list their actual inbound and
outbound rules; instances show AMI, root device, attached volumes, SSM
agent state and ready-to-paste `ssh`/`scp` commands.

## Drill-down

`Enter` on a **vpc**, **subnet** or **security group** jumps to the
instances inside it: the `ec2` view opens pre-filtered by that id.
`Esc` walks back to where you came from, cursor intact. `d` opens the
describe dashboard for those kinds too.
