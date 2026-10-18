//! Port of ReloadCommand's request boundary; pack discovery and resource reload belong to the host.
use super::{CommandSourceStack, builtins::Feedback};
use ferraloom_protocol::chat::Component;

#[derive(Clone, Debug)]
pub struct ReloadRequest {
    pub source: CommandSourceStack,
}
pub trait ReloadAccess {
    type Error;
    fn send_success(&mut self, source: &CommandSourceStack, feedback: Feedback);
    /// The host must perform the request or report it through the canonical unported policy.
    fn request_reload(&mut self, request: ReloadRequest) -> Result<(), Self::Error>;
}
/// Vanilla reports success before handing the owned source to its asynchronous reload operation.
pub fn run<A: ReloadAccess>(source: CommandSourceStack, access: &mut A) -> Result<i32, A::Error> {
    access.send_success(
        &source,
        Feedback {
            message: Component::translatable("commands.reload.success", vec![]),
            broadcast: true,
        },
    );
    access.request_reload(ReloadRequest { source })?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::Vec3;
    #[derive(Default)]
    struct Host(Vec<serde_json::Value>);
    impl ReloadAccess for Host {
        type Error = std::convert::Infallible;
        fn send_success(&mut self, _: &CommandSourceStack, feedback: Feedback) {
            self.0.push(serde_json::json!({
                "feedback":serde_json::from_str::<serde_json::Value>(&feedback.message.to_json().unwrap()).unwrap(),
                "broadcast":feedback.broadcast,
            }));
        }
        fn request_reload(&mut self, request: ReloadRequest) -> Result<(), Self::Error> {
            self.0
                .push(serde_json::json!({"reload":request.source.server}));
            Ok(())
        }
    }
    #[test]
    fn request_order_feedback_and_return_value_match_java() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("vanilla/reload-vector.json")).unwrap();
        let mut source = CommandSourceStack::server(4, Vec3::ZERO);
        source.server = 17;
        let mut host = Host::default();
        let value = run(source, &mut host).unwrap();
        assert_eq!(serde_json::json!({"value":value,"events":host.0}), expected);
    }
}
