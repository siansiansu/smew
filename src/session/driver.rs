//! PluginDriver drives sessions via the aws CLI + session-manager-plugin.

/// Configures SSH-over-SSM config generation.
#[derive(Debug, Clone)]
pub struct SshOptions {
    /// Pushes a 60s temporary public key per connection via EC2 Instance
    /// Connect (no permanent authorized_keys entry). Default mode.
    pub ephemeral: bool,
    /// Local public key path used in ephemeral mode.
    pub public_key: String,
}

/// Drives sessions via the aws CLI + session-manager-plugin.
#[derive(Debug, Clone)]
pub struct PluginDriver {
    profile: String,
    region: String,
}

impl PluginDriver {
    /// Returns a driver bound to a profile/region (either may be empty to
    /// defer to the aws CLI's own resolution).
    pub fn new(profile: &str, region: &str) -> Self {
        Self {
            profile: profile.to_string(),
            region: region.to_string(),
        }
    }

    /// The `--region` / `--profile` pairs to append to every aws invocation
    /// (a flag is omitted when its value is empty, deferring to the CLI).
    fn flag_pairs(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("--region", self.region.as_str()),
            ("--profile", self.profile.as_str()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
    }

    /// Builds the argv for
    /// `aws ssm start-session --target <id> [--region ..] [--profile ..]`.
    pub fn shell_command(&self, target: &str) -> Vec<String> {
        let mut args: Vec<String> = ["aws", "ssm", "start-session", "--target", target]
            .map(String::from)
            .into();
        for (flag, value) in self.flag_pairs() {
            args.push(flag.into());
            args.push(value.into());
        }
        args
    }

    /// Builds the argv for an SSM port-forwarding session. With a remote
    /// host it forwards through the instance to that host
    /// (AWS-StartPortForwardingSessionToRemoteHost); otherwise to a port on
    /// the instance itself (AWS-StartPortForwardingSession). The parameters
    /// are a single JSON argv element — no shell is involved — but the
    /// caller must validate `remote_host` (the form restricts its charset).
    pub fn port_forward_command(
        &self,
        target: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> Vec<String> {
        let (doc, params) = if remote_host.is_empty() {
            (
                "AWS-StartPortForwardingSession",
                format!(r#"{{"portNumber":["{remote_port}"],"localPortNumber":["{local_port}"]}}"#),
            )
        } else {
            (
                "AWS-StartPortForwardingSessionToRemoteHost",
                format!(
                    r#"{{"host":["{remote_host}"],"portNumber":["{remote_port}"],"localPortNumber":["{local_port}"]}}"#
                ),
            )
        };
        let mut args: Vec<String> = [
            "aws",
            "ssm",
            "start-session",
            "--target",
            target,
            "--document-name",
            doc,
            "--parameters",
            &params,
        ]
        .map(String::from)
        .into();
        for (flag, value) in self.flag_pairs() {
            args.push(flag.into());
            args.push(value.into());
        }
        args
    }

    fn aws_flags(&self) -> String {
        let mut s = String::new();
        for (flag, value) in self.flag_pairs() {
            s.push(' ');
            s.push_str(flag);
            s.push(' ');
            s.push_str(value);
        }
        s
    }

    /// Builds the ssh ProxyCommand that tunnels SSH over SSM via the
    /// AWS-StartSSHSession document. ssh substitutes %h (instance id),
    /// %p (port), and %r (login user). No inbound port is opened.
    ///
    /// In ephemeral mode it first pushes a 60-second public key via EC2
    /// Instance Connect (its JSON output is discarded so it doesn't corrupt
    /// the SSH stream).
    pub fn ssh_proxy_command(&self, opt: &SshOptions) -> String {
        let ssm = format!(
            "aws ssm start-session --target %h --document-name AWS-StartSSHSession --parameters 'portNumber=%p'{}",
            self.aws_flags()
        );
        if opt.ephemeral {
            let push = format!(
                "aws ec2-instance-connect send-ssh-public-key --instance-id %h --instance-os-user %r --ssh-public-key 'file://{}'{} >/dev/null",
                opt.public_key,
                self.aws_flags()
            );
            return format!("sh -c \"{push} && {ssm}\"");
        }
        format!("sh -c \"{ssm}\"")
    }

    /// Returns a ~/.ssh/config block matching instance ids so that
    /// ssh/scp/rsync/sftp/VSCode-Remote work through SSM (no open port 22).
    pub fn ssh_config_block(&self, opt: &SshOptions) -> String {
        let mut tag = String::from("skua");
        if opt.ephemeral {
            tag.push_str(" · ephemeral (EC2 Instance Connect)");
        } else {
            tag.push_str(" · static key");
        }
        if !self.profile.is_empty() {
            tag.push_str(" · profile ");
            tag.push_str(&self.profile);
        }
        if !self.region.is_empty() {
            tag.push_str(" · region ");
            tag.push_str(&self.region);
        }
        format!(
            "# {tag}\nHost i-* mi-*\n  ProxyCommand {}\n",
            self.ssh_proxy_command(opt)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_args() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        assert_eq!(
            d.shell_command("i-0abc"),
            vec![
                "aws",
                "ssm",
                "start-session",
                "--target",
                "i-0abc",
                "--region",
                "ap-northeast-1",
                "--profile",
                "prod"
            ]
        );
        let d = PluginDriver::new("", "");
        assert_eq!(
            d.shell_command("i-1"),
            vec!["aws", "ssm", "start-session", "--target", "i-1"]
        );
    }

    #[test]
    fn port_forward_command_args() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        assert_eq!(
            d.port_forward_command("i-0abc", 15432, "db.internal", 5432),
            vec![
                "aws",
                "ssm",
                "start-session",
                "--target",
                "i-0abc",
                "--document-name",
                "AWS-StartPortForwardingSessionToRemoteHost",
                "--parameters",
                r#"{"host":["db.internal"],"portNumber":["5432"],"localPortNumber":["15432"]}"#,
                "--region",
                "ap-northeast-1",
                "--profile",
                "prod"
            ]
        );
        // No remote host → forward to the instance itself.
        let d = PluginDriver::new("", "");
        assert_eq!(
            d.port_forward_command("i-1", 8080, "", 80),
            vec![
                "aws",
                "ssm",
                "start-session",
                "--target",
                "i-1",
                "--document-name",
                "AWS-StartPortForwardingSession",
                "--parameters",
                r#"{"portNumber":["80"],"localPortNumber":["8080"]}"#,
            ]
        );
    }

    #[test]
    fn ssh_config_block_static() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let block = d.ssh_config_block(&SshOptions {
            ephemeral: false,
            public_key: String::new(),
        });
        assert_eq!(
            block,
            "# skua · static key · profile prod · region ap-northeast-1\n\
             Host i-* mi-*\n\
             \x20 ProxyCommand sh -c \"aws ssm start-session --target %h --document-name AWS-StartSSHSession --parameters 'portNumber=%p' --region ap-northeast-1 --profile prod\"\n"
        );
    }

    #[test]
    fn ssh_config_block_ephemeral() {
        let d = PluginDriver::new("", "");
        let block = d.ssh_config_block(&SshOptions {
            ephemeral: true,
            public_key: "/home/u/.ssh/id_ed25519.pub".into(),
        });
        assert!(block.contains(
            "ProxyCommand sh -c \"aws ec2-instance-connect send-ssh-public-key --instance-id %h --instance-os-user %r --ssh-public-key 'file:///home/u/.ssh/id_ed25519.pub' >/dev/null && aws ssm start-session --target %h"
        ));
        assert!(block.starts_with("# skua · ephemeral (EC2 Instance Connect)\n"));
    }
}
