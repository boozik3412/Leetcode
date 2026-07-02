use crate::config::AppConfig;
use anyhow::Context;

pub fn build_http_client(config: &AppConfig) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();

    if !config.proxy_use_system {
        builder = builder.no_proxy();
    }

    if config.proxy_enabled {
        let proxy_url = config.manual_proxy_url();
        let Some(proxy_url) = proxy_url.as_deref() else {
            anyhow::bail!("Proxy включен, но адрес proxy пуст");
        };

        let mut proxy = reqwest::Proxy::all(proxy_url)
            .with_context(|| format!("Некорректный proxy URL: {proxy_url}"))?;
        if !config.proxy_username.trim().is_empty() {
            proxy = proxy.basic_auth(config.proxy_username.trim(), &config.proxy_password);
        }
        if !config.proxy_no_proxy.trim().is_empty() {
            proxy = proxy.no_proxy(reqwest::NoProxy::from_string(config.proxy_no_proxy.trim()));
        }
        builder = builder.proxy(proxy);
    }

    builder.build().context("Не удалось создать HTTP-клиент")
}

pub fn proxy_status_label(config: &AppConfig) -> String {
    if config.proxy_enabled {
        config
            .manual_proxy_url()
            .unwrap_or_else(|| "ручной proxy без адреса".to_string())
    } else {
        "ручной proxy выключен".to_string()
    }
}

pub fn proxy_system_status_label(config: &AppConfig) -> &'static str {
    if config.proxy_use_system {
        "системные proxy/env включены"
    } else {
        "системные proxy/env отключены"
    }
}
