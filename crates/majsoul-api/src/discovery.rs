use crate::{ApiError, Policy, policy::Pacer};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub(crate) const ORIGIN: &str = "https://game.maj-soul.com";
pub(crate) const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub package_version: String,
    pub resource_version: String,
    pub endpoint: String,
    pub route_id: String,
}

fn quoted_value(text: &str, marker: &str) -> Result<String> {
    let value = text.split_once(marker).context("客户端页面缺少版本信息")?.1;
    let value = value
        .split_once(':')
        .context("客户端版本格式无效")?
        .1
        .trim_start();
    let quote = value.chars().next().context("客户端版本为空")?;
    ensure!(quote == '\'' || quote == '"', "客户端版本格式无效");
    Ok(value[1..]
        .split_once(quote)
        .context("客户端版本格式无效")?
        .0
        .into())
}

async fn get(http: &reqwest::Client, pacer: &mut Pacer, url: &str) -> Result<Vec<u8>> {
    for attempt in 0..=1 {
        pacer.wait().await;
        let response = http.get(url).send().await;
        match response {
            Ok(response) => {
                if response.status().as_u16() == 429 {
                    let seconds = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok()?.parse().ok())
                        .unwrap_or(60);
                    return Err(ApiError::Cooldown { seconds }.into());
                }
                if response.status().is_server_error() && attempt == 0 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                let response = response.error_for_status()?;
                ensure!(
                    response.content_length().unwrap_or(0) <= 16 * 1024 * 1024,
                    "网关配置过大"
                );
                let bytes = response.bytes().await?;
                ensure!(bytes.len() <= 16 * 1024 * 1024, "客户端元数据过大");
                return Ok(bytes.to_vec());
            }
            Err(error) if attempt == 0 && (error.is_timeout() || error.is_connect()) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

pub(crate) fn select_route(response: &Value) -> Result<(String, String)> {
    let data = &response["data"];
    if data["maintenance"].is_object() || data["maintenance"].as_bool() == Some(true) {
        return Err(ApiError::Maintenance.into());
    }
    let mut routes = data["routes"]
        .as_array()
        .context("网关未提供线路")?
        .iter()
        .filter(|r| r["ssl"] == true && r["state"] != "removed" && r["state"] != "banned")
        .collect::<Vec<_>>();
    routes.sort_by_key(|r| {
        (
            r["level"].as_u64().unwrap_or(0),
            r["order"].as_u64().unwrap_or(1000),
        )
    });
    let route = routes.first().context("没有可用的安全线路")?;
    let id = route["id"].as_str().context("线路缺少 ID")?;
    let domain = route["domain"].as_str().context("线路缺少地址")?;
    let endpoint = format!("wss://{domain}/gateway");
    let parsed = reqwest::Url::parse(&endpoint)?;
    ensure!(
        parsed
            .host_str()
            .is_some_and(|h| h.ends_with(".maj-soul.com"))
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.path() == "/gateway"
            && parsed.query().is_none(),
        "登录线路地址无效"
    );
    Ok((id.into(), endpoint))
}

fn preferred_url(entries: &Value) -> Result<String> {
    let entry = entries
        .as_array()
        .context("客户端缺少地址列表")?
        .iter()
        .filter(|e| e["url"].as_str().is_some_and(|u| u.starts_with("https://")))
        .max_by_key(|e| {
            (
                e["Priority"].as_i64().unwrap_or(0),
                e["weight"].as_i64().unwrap_or(0),
            )
        })
        .context("客户端缺少安全地址")?;
    Ok(entry["url"].as_str().unwrap().into())
}
fn join_url(base: &str, path: &str) -> Result<String> {
    let base = reqwest::Url::parse(&format!("{}/", base.trim_end_matches('/')))?;
    let url = base.join(path)?;
    ensure!(url.scheme() == "https", "客户端元数据必须使用 HTTPS");
    Ok(url.into())
}

/// Discover current CN Unity version and routing. This does not log in.
static DISCOVERY: tokio::sync::Mutex<Option<Discovery>> = tokio::sync::Mutex::const_new(None);

/// Reuse public version/route metadata for this process. The lock also joins prefetch.
pub async fn discover(policy: &Policy) -> Result<Discovery> {
    let mut cached = DISCOVERY.lock().await;
    if let Some(discovery) = cached.as_ref() {
        return Ok(discovery.clone());
    }
    let discovery = discover_uncached(policy).await?;
    *cached = Some(discovery.clone());
    Ok(discovery)
}

pub async fn invalidate_discovery() {
    *DISCOVERY.lock().await = None;
}

async fn discover_uncached(policy: &Policy) -> Result<Discovery> {
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(policy.timeout)
        .connect_timeout(policy.timeout)
        .retry(reqwest::retry::never())
        .build()?;
    let mut pacer = Pacer::new(policy.request_interval);
    let html = String::from_utf8(get(&http, &mut pacer, &format!("{ORIGIN}/1/")).await?)?;
    let package_version = quoted_value(&html, "productVersion")?;
    ensure!(
        package_version
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.'),
        "客户端版本格式无效"
    );
    let issuer = html
        .split("Build/")
        .skip(1)
        .find_map(|s| s.split_once("-WebGL-release-").map(|(issuer, _)| issuer))
        .context("客户端入口缺少 Unity 发布标识")?;
    ensure!(
        !issuer.is_empty()
            && issuer
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "客户端发布标识无效"
    );
    let settings: Value = serde_json::from_slice(
        &get(
            &http,
            &mut pacer,
            &format!("{ORIGIN}/assetbundles/clientBundleSettings/{issuer}-release.json"),
        )
        .await?,
    )?;
    let warehouse = settings["warehouses"]
        .as_array()
        .and_then(|a| a.first())
        .context("客户端缺少仓库配置")?;
    let settings_url = join_url(
        &preferred_url(&warehouse["urls"])?,
        warehouse["warehouseSettingPath"]
            .as_str()
            .context("客户端缺少仓库地址")?,
    )?;
    let settings: Value = serde_json::from_slice(&get(&http, &mut pacer, &settings_url).await?)?;
    let base = join_url(
        &preferred_url(&settings["urls"])?,
        settings["bundlePath"]
            .as_str()
            .context("客户端缺少资源根路径")?,
    )?;
    let base = format!("{}/ASTC/", base.trim_end_matches('/'));
    let info = crate::metadata::read_json(
        get(&http, &mut pacer, &format!("{base}bundle_info_so.majset")).await?,
        None,
    )?;
    let owners = crate::metadata::version_bundles(&info)?;
    let mut configuration = None;
    let mut resource_version = None;
    for (path, bundle) in owners {
        let value = crate::metadata::read_json(
            get(&http, &mut pacer, &format!("{base}{bundle}")).await?,
            Some(&path),
        )?;
        if path.ends_with("docs/config.json") {
            configuration = Some(value);
        } else {
            resource_version = value["version"].as_str().map(str::to_owned);
        }
    }
    let resource_version = resource_version.context("Unity 资源版本缺失")?;
    ensure!(
        !resource_version.is_empty()
            && resource_version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.'),
        "资源版本格式无效"
    );
    let config = configuration.context("Unity 网关配置缺失")?;
    let player = config["ip"]
        .as_array()
        .and_then(|ips| ips.iter().find(|i| i["gateways"].is_array()))
        .context("配置缺少账号网关")?;
    let gateways = player["gateways"]
        .as_array()
        .context("网关配置已变化，需要更新连接适配")?;
    let mut last_error = None;
    // Bound failover as well as retries; never fan out connections to all routes.
    for gateway in gateways
        .iter()
        .filter(|g| g["url"].as_str().is_some_and(|u| u.starts_with("https://")))
        .take(2)
    {
        let base = gateway["url"].as_str().context("网关地址缺失")?;
        let base_url = reqwest::Url::parse(base)?;
        ensure!(
            base_url.scheme() == "https"
                && base_url
                    .host_str()
                    .is_some_and(|h| h.ends_with(".maj-soul.com")),
            "网关地址无效"
        );
        let url = format!(
            "{}/api/clientgate/routes?platform=Web&version={package_version}&lang=chs_t",
            base.trim_end_matches('/')
        );
        match get(&http, &mut pacer, &url).await {
            Ok(bytes) => {
                let response: Value = serde_json::from_slice(&bytes)?;
                let (route_id, endpoint) = select_route(&response)?;
                return Ok(Discovery {
                    package_version,
                    resource_version,
                    endpoint,
                    route_id,
                });
            }
            Err(error) if error.downcast_ref::<ApiError>().is_some() => return Err(error),
            Err(error) => last_error = Some(error),
        }
    }
    bail!(
        "无法获取登录线路：{}",
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "配置中没有网关".into())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn routes_use_secure_endpoints_and_server_order() -> Result<()> {
        let (id, endpoint) = select_route(&json!({"data":{"routes":[
            {"id":"blocked","domain":"bad.maj-soul.com","ssl":false,"order":0},
            {"id":"second","domain":"route-3.maj-soul.com:8443","ssl":true,"order":3},
            {"id":"first","domain":"route-2.maj-soul.com:443","ssl":true,"order":2}
        ]}}))?;
        assert_eq!(id, "first");
        assert_eq!(endpoint, "wss://route-2.maj-soul.com:443/gateway");
        assert!(
            select_route(
                &json!({"data":{"routes":[{"id":"x","domain":"example.com","ssl":true}]}})
            )
            .is_err()
        );
        assert_eq!(
            quoted_value("productVersion: '4.0.51',", "productVersion")?,
            "4.0.51"
        );
        Ok(())
    }
}
