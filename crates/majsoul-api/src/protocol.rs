use anyhow::{Context, Result, ensure};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, SerializeOptions};
use serde_json::Value;
use std::sync::LazyLock;

static DESCRIPTORS: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(include_bytes!(concat!(env!("OUT_DIR"), "/liqi.bin")).as_slice())
        .expect("bundled Liqi descriptors must be valid")
});

#[derive(Clone, PartialEq, Message)]
pub(crate) struct Wrapper {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(bytes = "vec", tag = "2")]
    pub data: Vec<u8>,
}

/// Account reads plus the minimum connection and authentication lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Connect,
    Heartbeat,
    Login,
    LoginBeat,
    LoginSuccess,
    Account,
    Characters,
    Bag,
    Titles,
}
impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Self::Connect => "lq.Route.requestConnection",
            Self::Heartbeat => "lq.Route.heartbeat",
            Self::Login => "lq.Lobby.login",
            Self::LoginBeat => "lq.Lobby.loginBeat",
            Self::LoginSuccess => "lq.Lobby.loginSuccess",
            Self::Account => "lq.Lobby.fetchAccountInfo",
            Self::Characters => "lq.Lobby.fetchCharacterInfo",
            Self::Bag => "lq.Lobby.fetchBagInfo",
            Self::Titles => "lq.Lobby.fetchTitleList",
        }
    }
    fn descriptor(self) -> Result<prost_reflect::MethodDescriptor> {
        let (service, method) = self.name().rsplit_once('.').unwrap();
        DESCRIPTORS
            .get_service_by_name(service)
            .and_then(|s| s.methods().find(|m| m.name() == method))
            .with_context(|| format!("协议缺少 {}", self.name()))
    }
    pub fn request(self, sequence: u16, payload: &Value) -> Result<Vec<u8>> {
        let message = DynamicMessage::deserialize(self.descriptor()?.input(), payload)?;
        let wrapper = Wrapper {
            name: format!(".{}", self.name()),
            data: message.encode_to_vec(),
        };
        let mut bytes = vec![2];
        bytes.extend_from_slice(&sequence.to_le_bytes());
        wrapper.encode(&mut bytes)?;
        Ok(bytes)
    }
    pub fn response(self, payload: &[u8]) -> Result<Value> {
        let message = DynamicMessage::decode(self.descriptor()?.output(), payload)?;
        let options = SerializeOptions::new()
            .use_proto_field_name(true)
            .skip_default_fields(false);
        message
            .serialize_with_options(serde_json::value::Serializer, &options)
            .context("无法解析响应字段")
    }
    #[cfg(test)]
    pub fn mock_response(self, sequence: u16, payload: &Value) -> Result<Vec<u8>> {
        let message = DynamicMessage::deserialize(self.descriptor()?.output(), payload)?;
        let mut bytes = vec![3];
        bytes.extend_from_slice(&sequence.to_le_bytes());
        Wrapper {
            name: String::new(),
            data: message.encode_to_vec(),
        }
        .encode(&mut bytes)?;
        Ok(bytes)
    }
}

pub(crate) fn response_frame(bytes: &[u8], sequence: u16) -> Result<Option<Wrapper>> {
    ensure!(!bytes.is_empty(), "收到空协议消息");
    if bytes[0] == 1 {
        let wrapper = Wrapper::decode(&bytes[1..]).context("推送消息封装无效")?;
        ensure!(
            !matches!(
                wrapper.name.as_str(),
                ".lq.NotifyAnotherLogin"
                    | ".lq.NotifyAccountLogout"
                    | ".lq.NotifyConnectionShutdown"
            ),
            "服务器已结束账号会话，请手动重新登录；已同步的数据已保留"
        );
        return Ok(None);
    }
    ensure!(bytes.len() >= 3 && bytes[0] == 3, "响应消息头无效");
    ensure!(
        u16::from_le_bytes([bytes[1], bytes[2]]) == sequence,
        "响应序号不匹配"
    );
    let wrapper = Wrapper::decode(&bytes[3..])?;
    ensure!(wrapper.name.is_empty(), "响应中出现了意外的方法名");
    Ok(Some(wrapper))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn current_schema_encodes_login_and_route_handshake() -> Result<()> {
        let bytes = Method::Connect.request(
            0x1234,
            &json!({"type":1,"route_id":"route-2","timestamp":"123","platform":"Web"}),
        )?;
        assert_eq!(&bytes[..3], &[2, 0x34, 0x12]);
        let wrapper = Wrapper::decode(&bytes[3..])?;
        assert_eq!(wrapper.name, ".lq.Route.requestConnection");
        assert!(!wrapper.data.is_empty());
        Method::Login.request(1, &json!({"account":"test","password":"derived","device":{"is_browser":true},"client_version":{"package":"4.0.51"}}))?;
        Ok(())
    }
    #[test]
    fn notification_does_not_consume_the_outstanding_response() -> Result<()> {
        let mut notify = vec![1];
        Wrapper {
            name: ".lq.NotifyAccountUpdate".into(),
            data: vec![],
        }
        .encode(&mut notify)?;
        assert!(response_frame(&notify, 9)?.is_none());
        let mut response = vec![3, 9, 0];
        Wrapper {
            name: String::new(),
            data: vec![],
        }
        .encode(&mut response)?;
        assert!(response_frame(&response, 9)?.is_some());
        assert!(response_frame(&response, 8).is_err());
        assert!(response_frame(&[3, 1], 1).is_err());
        Ok(())
    }
    #[test]
    fn server_session_end_notifications_stop_the_connection() -> Result<()> {
        for name in [
            ".lq.NotifyAnotherLogin",
            ".lq.NotifyAccountLogout",
            ".lq.NotifyConnectionShutdown",
        ] {
            let mut bytes = vec![1];
            Wrapper {
                name: name.into(),
                data: vec![],
            }
            .encode(&mut bytes)?;
            assert!(response_frame(&bytes, 1).is_err());
        }
        Ok(())
    }
    #[test]
    fn every_exposed_method_resolves_in_the_current_protocol() -> Result<()> {
        for method in [
            Method::Connect,
            Method::Heartbeat,
            Method::Login,
            Method::LoginBeat,
            Method::LoginSuccess,
            Method::Account,
            Method::Characters,
            Method::Bag,
            Method::Titles,
        ] {
            method.descriptor()?;
        }
        Ok(())
    }
}
