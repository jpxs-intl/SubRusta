use std::net::SocketAddr;

#[derive(Debug)]
pub enum MsError {
    Http(reqwest::Error),
    BadInfo(String),
    BadAddr(String),
}

impl std::fmt::Display for MsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MsError::Http(e)     => write!(f, "master server HTTP error: {e}"),
            MsError::BadInfo(b)  => write!(f, "unparseable serverinfo response: {b:?}"),
            MsError::BadAddr(a)  => write!(f, "bad master server address: {a:?}"),
        }
    }
}
impl std::error::Error for MsError {}

pub async fn resolve_address(master_url: &str, master_ip: Option<&str>) -> Result<SocketAddr, MsError> {
    if let Some(ip) = master_ip {
        return ip.parse().map_err(|_| MsError::BadAddr(ip.to_string()));
    }

    let client = reqwest::Client::builder()
        .user_agent("SubRosa")
        .build()
        .map_err(MsError::Http)?;

    let body = client
        .get(info_url(master_url))
        .send()
        .await
        .map_err(MsError::Http)?
        .text()
        .await
        .map_err(MsError::Http)?;

    parse_server_info(&body)
}

fn info_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/anewzero/serverinfo.php");
    if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("http://{url}")
    }
}

fn parse_server_info(body: &str) -> Result<SocketAddr, MsError> {
    let fields: Vec<&str> = body.split('\t').filter(|s| !s.is_empty()).collect();

    let host = fields.get(1).ok_or_else(|| MsError::BadInfo(body.to_string()))?;
    let base_port: u16 = fields
        .get(2)
        .and_then(|p| p.trim().parse().ok())
        .ok_or_else(|| MsError::BadInfo(body.to_string()))?;

    let auth_port = base_port.checked_add(2).ok_or_else(|| MsError::BadInfo(body.to_string()))?;

    format!("{host}:{auth_port}")
        .parse()
        .map_err(|_| MsError::BadAddr(format!("{host}:{auth_port}")))
}