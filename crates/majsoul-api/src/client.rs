use crate::{
    ApiError, Discovery, Policy, discovery,
    policy::{Pacer, reserve_login},
    protocol::{Method, response_frame},
};
use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{
    net::TcpStream,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};
use zeroize::{Zeroize, Zeroizing};

/// Complete query results; authentication responses/tokens are deliberately excluded.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub region: String,
    pub uid: u32,
    pub synced_at: SystemTime,
    pub account: Value,
    pub characters: Value,
    pub bag: Value,
    pub titles: Value,
}

pub struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    discovery: Discovery,
    policy: Policy,
    pacer: Pacer,
    sequence: u16,
    uid: Option<u32>,
    last_sync: Option<Instant>,
    broken: bool,
    device_id: String,
}
impl Client {
    /// Establish a route connection only. No account request is sent here.
    pub async fn connect(discovery: Discovery, policy: Policy) -> Result<Self> {
        let mut request = discovery.endpoint.clone().into_client_request()?;
        request
            .headers_mut()
            .insert("Origin", discovery::ORIGIN.parse()?);
        request
            .headers_mut()
            .insert("User-Agent", discovery::USER_AGENT.parse()?);
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "ws".parse()?);
        let (socket, _) = timeout(policy.timeout, tokio_tungstenite::connect_async(request))
            .await
            .context("连接登录线路超时")??;
        let mut client = Self {
            socket,
            discovery,
            pacer: Pacer::new(policy.request_interval),
            policy,
            sequence: 0,
            uid: None,
            last_sync: None,
            broken: false,
            device_id: uuid::Uuid::new_v4().to_string(),
        };
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let response = client.rpc(Method::Connect, &json!({
            "type":1,"route_id":client.discovery.route_id,"timestamp":timestamp.to_string(),"platform":"Web"
        })).await?;
        ensure!(
            response["result"] == 1,
            "服务器暂未接受线路连接，请稍后手动重试"
        );
        Ok(client)
    }

    pub fn set_device_id(&mut self, device_id: &str) -> Result<()> {
        ensure!(self.uid.is_none(), "请在登录前设置设备标识");
        uuid::Uuid::parse_str(device_id).context("设备标识格式无效")?;
        self.device_id = device_id.into();
        Ok(())
    }

    pub async fn login(&mut self, username: &str, password: Zeroizing<String>) -> Result<u32> {
        let hash = derive_password(&password);
        drop(password);
        self.login_with_hash(username, hash).await
    }

    /// The HMAC value is a reusable credential and must be protected like a password.
    pub async fn login_with_hash(
        &mut self,
        username: &str,
        password: Zeroizing<String>,
    ) -> Result<u32> {
        ensure!(self.uid.is_none(), "当前连接已经登录");
        ensure!(
            !username.trim().is_empty() && !password.is_empty(),
            "请输入账号和密码"
        );
        reserve_login()?;
        ensure!(
            password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit()),
            "密码派生值格式无效"
        );
        let mut payload =
            login_payload_hashed(username.trim(), &password, &self.discovery, &self.device_id);
        drop(password);
        let response = self.rpc(Method::Login, &payload).await;
        if let Some(Value::String(mut password)) =
            payload.as_object_mut().and_then(|p| p.remove("password"))
        {
            password.zeroize();
        }
        let response = response?;
        let uid = response["account_id"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id > 0)
            .context("登录响应未提供账号 ID")?;
        self.uid = Some(uid);
        // Current Unity LoginMgr completes authentication with this contract.
        self.rpc(
            Method::LoginBeat,
            &json!({"contract":"DF2vkXCnfeXp4WoGrBGNcJBufZiMN3uP"}),
        )
        .await?;
        self.rpc(Method::LoginSuccess, &json!({})).await?;
        Ok(uid)
    }

    /// Sequential bounded queries. A failed round never returns a partial snapshot.
    pub async fn sync(&mut self) -> Result<Snapshot> {
        let uid = self.uid.context("请先登录")?;
        if let Some(previous) = self.last_sync {
            let remaining = self.policy.sync_interval.saturating_sub(previous.elapsed());
            if !remaining.is_zero() {
                return Err(ApiError::Cooldown {
                    seconds: remaining.as_secs() + 1,
                }
                .into());
            }
        }
        // Reserve even if a query fails, preventing a rapid manual retry loop.
        self.last_sync = Some(Instant::now());
        let profile = self
            .rpc(Method::Account, &json!({"account_id":uid}))
            .await?;
        let characters = self.rpc(Method::Characters, &json!({})).await?;
        let bag = self.rpc(Method::Bag, &json!({})).await?;
        let titles = self.rpc(Method::Titles, &json!({})).await?;
        ensure!(
            profile["account"]["account_id"].as_u64() == Some(uid as u64),
            "账号信息与当前会话不匹配"
        );
        Ok(Snapshot {
            region: "cn".into(),
            uid,
            synced_at: SystemTime::now(),
            account: profile["account"].clone(),
            characters,
            bag,
            titles,
        })
    }

    pub async fn heartbeat(&mut self) -> Result<()> {
        self.rpc(
            Method::Heartbeat,
            &json!({"platform":11,"no_operation_counter":0}),
        )
        .await?;
        Ok(())
    }
    pub fn heartbeat_interval(&self) -> Duration {
        self.policy.heartbeat_interval
    }
    pub fn is_connected(&self) -> bool {
        !self.broken
    }
    pub async fn close(&mut self) {
        let _ = timeout(Duration::from_secs(2), self.socket.close(None)).await;
        self.broken = true;
    }
    async fn rpc(&mut self, method: Method, payload: &Value) -> Result<Value> {
        ensure!(!self.broken, ApiError::Disconnected);
        self.pacer.wait().await;
        self.sequence = self.sequence.wrapping_add(1);
        let sequence = self.sequence;
        let request = method.request(sequence, payload)?;
        let result = timeout(self.policy.timeout, async {
            self.socket.send(Message::Binary(request.into())).await?;
            while let Some(message) = self.socket.next().await {
                match message? {
                    Message::Binary(bytes) => {
                        if let Some(wrapper) = response_frame(&bytes, sequence)? {
                            return method.response(&wrapper.data);
                        }
                    }
                    Message::Ping(_) => self.socket.flush().await?,
                    Message::Close(_) => return Err(ApiError::Disconnected.into()),
                    _ => {}
                }
            }
            Err(ApiError::Disconnected.into())
        })
        .await;
        let response = match result {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                self.broken = true;
                return Err(error.context(format!("{} 通信失败", method.name())));
            }
            Err(_) => {
                self.broken = true;
                anyhow::bail!("{} 请求超时，请手动重连", method.name());
            }
        };
        let code = response["error"]["code"].as_u64().unwrap_or(0);
        if code != 0 {
            return Err(ApiError::Server {
                method: method.name().into(),
                code,
            }
            .into());
        }
        Ok(response)
    }
}

pub fn derive_password(password: &str) -> Zeroizing<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(b"lailai").expect("HMAC accepts this key");
    mac.update(password.as_bytes());
    Zeroizing::new(
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

#[cfg(test)]
fn login_payload(
    username: &str,
    password: &str,
    discovery: &Discovery,
    device_id: &str,
) -> Result<Value> {
    Ok(login_payload_hashed(
        username,
        &derive_password(password),
        discovery,
        device_id,
    ))
}

fn login_payload_hashed(
    username: &str,
    password: &str,
    discovery: &Discovery,
    device_id: &str,
) -> Value {
    // Login tabs use 0 for account/email and 1 for phone; signup uses 1/2.
    let login_type = if username.len() == 11
        && username.starts_with('1')
        && username.bytes().all(|b| b.is_ascii_digit())
    {
        1
    } else {
        0
    };
    json!({
        "account":username, "password":password, "reconnect":false,
        "gen_access_token":false, "random_key":device_id,
        "currency_platforms":[2], "type":login_type,
        "client_version":{"resource":discovery.resource_version,"package":discovery.package_version},
        "client_version_string":format!("WebGL_2022-{}",discovery.resource_version),
        "device":{"platform":"pc","hardware":"unknown","os":"windows","is_browser":true,
            "sale_platform":"web","user_agent":discovery::USER_AGENT,"screen_type":1}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // tungstenite mandates this response type for handshake callbacks.
    #[allow(clippy::result_large_err)]
    fn accept_handshake(
        request: &tokio_tungstenite::tungstenite::handshake::server::Request,
        mut response: tokio_tungstenite::tungstenite::handshake::server::Response,
    ) -> Result<
        tokio_tungstenite::tungstenite::handshake::server::Response,
        tokio_tungstenite::tungstenite::handshake::server::ErrorResponse,
    > {
        assert_eq!(request.headers()["Origin"], discovery::ORIGIN);
        response
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "ws".parse().unwrap());
        Ok(response)
    }
    #[test]
    fn authentication_payload_uses_hmac_and_current_unity_version() -> Result<()> {
        let discovery = Discovery {
            package_version: "4.0.51".into(),
            resource_version: "0.16.289".into(),
            endpoint: String::new(),
            route_id: String::new(),
        };
        let payload = login_payload(
            "account",
            "password",
            &discovery,
            "11111111-1111-4111-8111-111111111111",
        )?;
        assert_eq!(
            payload["password"],
            "9cf8f0f148916a48ce4384991b0f90422dc1c061dab59b54b37e04edf5de5adb"
        );
        assert_eq!(payload["client_version_string"], "WebGL_2022-0.16.289");
        assert_eq!(payload["gen_access_token"], false);
        Method::Login.request(1, &payload)?;
        assert_eq!(
            login_payload("one@example.com", "password", &discovery, "device")?["type"],
            0
        );
        assert_eq!(
            login_payload("13800000000", "password", &discovery, "device")?["type"],
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn peer_shutdown_marks_connection_unusable_immediately() -> Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            let mut socket = tokio_tungstenite::accept_hdr_async(stream, accept_handshake).await?;
            let Message::Binary(bytes) = socket.next().await.context("missing route request")??
            else {
                anyhow::bail!("unexpected frame");
            };
            let sequence = u16::from_le_bytes([bytes[1], bytes[2]]);
            socket
                .send(Message::Binary(
                    Method::Connect
                        .mock_response(sequence, &json!({"result":1}))?
                        .into(),
                ))
                .await?;
            let _ = socket.next().await;
            socket.close(None).await?;
            Ok::<_, anyhow::Error>(())
        });
        let mut client = Client::connect(
            Discovery {
                endpoint: format!("ws://{address}/gateway"),
                route_id: "local".into(),
                package_version: "test".into(),
                resource_version: "test".into(),
            },
            Policy {
                request_interval: Duration::from_millis(1),
                timeout: Duration::from_secs(3),
                ..Default::default()
            },
        )
        .await?;
        assert!(client.is_connected());
        assert!(client.heartbeat().await.is_err());
        assert!(!client.is_connected());
        server.await??;
        Ok(())
    }

    #[tokio::test]
    async fn local_server_checks_handshake_snapshot_and_sync_cooldown() -> Result<()> {
        use crate::protocol::Wrapper;
        use prost::Message as _;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            let mut socket = tokio_tungstenite::accept_hdr_async(stream, accept_handshake).await?;
            let responses = [
                (Method::Connect, json!({"result":1})),
                (
                    Method::Login,
                    json!({"account_id":42,"account":{"account_id":42}}),
                ),
                (Method::LoginBeat, json!({})),
                (Method::LoginSuccess, json!({})),
                (
                    Method::Account,
                    json!({"account":{"account_id":42,"nickname":"fixture"}}),
                ),
                (
                    Method::Characters,
                    json!({"characters":[{"charid":200001,"level":2,"exp":4000}]}),
                ),
                (
                    Method::Bag,
                    json!({"bag":{"items":[{"item_id":302002,"stack":10}]}}),
                ),
                (Method::Titles, json!({"title_list":[600001]})),
                (Method::Heartbeat, json!({})),
                (Method::Heartbeat, json!({})),
            ];
            for (method, payload) in responses {
                let message = socket.next().await.context("missing request")??;
                let Message::Binary(bytes) = message else {
                    anyhow::bail!("unexpected frame");
                };
                assert_eq!(bytes[0], 2);
                let sequence = u16::from_le_bytes([bytes[1], bytes[2]]);
                assert_eq!(
                    Wrapper::decode(&bytes[3..])?.name,
                    format!(".{}", method.name())
                );
                let mut notification = vec![1];
                Wrapper {
                    name: ".lq.NotifyAccountUpdate".into(),
                    data: vec![],
                }
                .encode(&mut notification)?;
                socket.send(Message::Binary(notification.into())).await?;
                socket
                    .send(Message::Binary(
                        method.mock_response(sequence, &payload)?.into(),
                    ))
                    .await?;
            }
            // The client must reject an immediate repeat without sending another request.
            let message = socket.next().await.context("missing close")??;
            assert!(matches!(message, Message::Close(_)));
            Ok::<_, anyhow::Error>(())
        });
        let policy = Policy {
            request_interval: Duration::from_millis(1),
            timeout: Duration::from_secs(3),
            ..Default::default()
        };
        let mut client = Client::connect(
            Discovery {
                endpoint: format!("ws://{address}/gateway"),
                route_id: "local".into(),
                package_version: "4.0.51".into(),
                resource_version: "test".into(),
            },
            policy,
        )
        .await?;
        client
            .login("fixture", Zeroizing::new("fixture-password".into()))
            .await?;
        let snapshot = client.sync().await?;
        assert_eq!(snapshot.uid, 42);
        assert_eq!(snapshot.bag["bag"]["items"][0]["stack"], 10);
        assert!(matches!(
            client.sync().await.unwrap_err().downcast_ref::<ApiError>(),
            Some(ApiError::Cooldown { .. })
        ));
        // Keep the authenticated connection alive without sending another login.
        client.heartbeat().await?;
        client.heartbeat().await?;
        client.close().await;
        server.await??;
        Ok(())
    }
}
