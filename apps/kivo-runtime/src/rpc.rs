//! Requests from the UIs (ARCHITECTURE §3). `hello`, `ping` and `state.get` are answered by the
//! IPC server itself; everything else arrives here. Every client is untrusted (INT-11): requests
//! carry no authority beyond what a user could do from the tray.

use crate::core::Core;
use kivo_ipc::{BoxFuture, Handler, RpcError, method};
use serde_json::Value;
use std::sync::Arc;

pub struct Rpc {
    core: Arc<Core>,
}

impl Rpc {
    pub fn new(core: Arc<Core>) -> Self {
        Self { core }
    }
}

impl Handler for Rpc {
    fn call(&self, name: String, _params: Value) -> BoxFuture<Result<Value, RpcError>> {
        let core = Arc::clone(&self.core);
        Box::pin(async move {
            let refused = |e: crate::core::Refused| RpcError::new(RpcError::REFUSED, e.0);
            match name.as_str() {
                method::SESSION_PAUSE => core.pause().map(|_| Value::Null).map_err(refused),
                method::SESSION_RESUME => core.resume().map(|_| Value::Null).map_err(refused),
                method::RUNTIME_QUIT => {
                    core.quit();
                    Ok(Value::Null)
                }
                _ => Err(RpcError::method_not_found(&name)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::SessionState;

    async fn call(rpc: &Rpc, name: &str) -> Result<Value, RpcError> {
        rpc.call(name.into(), Value::Null).await
    }

    #[tokio::test]
    async fn pause_resume_and_quit_reach_the_core() {
        let core = Arc::new(Core::new());
        let rpc = Rpc::new(Arc::clone(&core));
        call(&rpc, method::SESSION_PAUSE).await.unwrap();
        assert_eq!(core.session(), SessionState::Paused);
        let again = call(&rpc, method::SESSION_PAUSE).await.unwrap_err();
        assert_eq!(again.code, RpcError::REFUSED);
        call(&rpc, method::SESSION_RESUME).await.unwrap();
        assert_eq!(core.session(), SessionState::Idle);
        call(&rpc, method::RUNTIME_QUIT).await.unwrap();
        assert!(core.shutdown().is_cancelled());
    }

    #[tokio::test]
    async fn unknown_methods_are_refused() {
        let rpc = Rpc::new(Arc::new(Core::new()));
        let err = call(&rpc, "tools.run").await.unwrap_err();
        assert_eq!(err.code, RpcError::METHOD_NOT_FOUND);
    }
}
