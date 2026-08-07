//! 有界原始请求与发送前安全清理。

use crate::{
    IngressReject, MAX_HEADER_COUNT, MAX_HEADER_NAME_BYTES, MAX_HEADER_VALUE_BYTES,
    MAX_RAW_BODY_BYTES, MAX_REQUEST_TARGET_BYTES,
};
use zeroize::Zeroize;

const RESERVED_PROOF_HEADERS: &[&[u8]] = &[
    b"x-bianma-context-attestation",
    b"x-bianma-authorization-bundle",
    b"x-bianma-capability-authorization",
    b"x-bianma-ingress-mode",
];

const INBOUND_AUTH_HEADERS: &[&[u8]] = &[
    b"authorization",
    b"proxy-authorization",
    b"x-api-key",
    b"x-goog-api-key",
];

const STRIPPED_SENSITIVE_HEADERS: &[&[u8]] = &[b"cookie", b"set-cookie"];

const HOP_BY_HOP_HEADERS: &[&[u8]] = &[
    b"connection",
    b"keep-alive",
    b"proxy-connection",
    b"te",
    b"trailer",
    b"transfer-encoding",
    b"upgrade",
];

/// 支持的入站 HTTP 方法闭集。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HttpMethod {
    /// HTTP GET。
    Get,
    /// HTTP POST。
    Post,
    /// HTTP PUT。
    Put,
    /// HTTP PATCH。
    Patch,
    /// HTTP DELETE。
    Delete,
    /// HTTP HEAD。
    Head,
    /// HTTP OPTIONS。
    Options,
}

impl HttpMethod {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Get => 1,
            Self::Post => 2,
            Self::Put => 3,
            Self::Patch => 4,
            Self::Delete => 5,
            Self::Head => 6,
            Self::Options => 7,
        }
    }

    pub(crate) fn from_code(code: u8) -> Result<Self, IngressReject> {
        match code {
            1 => Ok(Self::Get),
            2 => Ok(Self::Post),
            3 => Ok(Self::Put),
            4 => Ok(Self::Patch),
            5 => Ok(Self::Delete),
            6 => Ok(Self::Head),
            7 => Ok(Self::Options),
            _ => Err(IngressReject::ProofMalformed),
        }
    }
}

/// 已完成基础语法校验并规范化名称大小写的原始 Header。
///
/// 值可能包含用户输入，因此本类型不实现 `Debug`。
pub struct RawHeader {
    pub(crate) name: Vec<u8>,
    pub(crate) value: Vec<u8>,
}

impl RawHeader {
    /// 构造 Header。名称被规范化为 ASCII 小写，值保持逐字节不变。
    pub fn try_new(name: &[u8], value: &[u8]) -> Result<Self, IngressReject> {
        if name.is_empty()
            || name.len() > MAX_HEADER_NAME_BYTES
            || value.len() > MAX_HEADER_VALUE_BYTES
            || !name.iter().copied().all(is_header_name_byte)
            || value
                .iter()
                .copied()
                .any(|byte| (byte < 0x20 && byte != b'\t') || byte == 0x7f)
        {
            return Err(IngressReject::HeaderMalformed);
        }

        let normalized_name = name.iter().map(u8::to_ascii_lowercase).collect();
        Ok(Self {
            name: normalized_name,
            value: value.to_vec(),
        })
    }

    /// 返回规范化后的 Header 名称。
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// 返回未经 trim 或重编码的 Header 值。
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

impl Drop for RawHeader {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

/// 入站 Gateway 已有界收集、但尚未取得任何路由权限的原始请求。
///
/// 本类型不实现 `Clone` 或 `Debug`，避免正文和认证输入被意外复制或记录。
pub struct RawIngressRequest {
    pub(crate) method: HttpMethod,
    pub(crate) target: Vec<u8>,
    pub(crate) headers: Vec<RawHeader>,
    pub(crate) body: Vec<u8>,
}

impl RawIngressRequest {
    /// 构造有界原始请求，并拒绝请求走私、证明混入和冲突认证输入。
    pub fn try_new(
        method: HttpMethod,
        target: &[u8],
        headers: Vec<RawHeader>,
        body: Vec<u8>,
    ) -> Result<Self, IngressReject> {
        validate_origin_form(target)?;
        if body.len() > MAX_RAW_BODY_BYTES {
            return Err(IngressReject::RequestTooLarge);
        }
        if headers.len() > MAX_HEADER_COUNT {
            return Err(IngressReject::RequestTooLarge);
        }

        let mut inbound_auth_count = 0usize;
        let mut content_length = None;
        let mut content_type_count = 0usize;

        for header in &headers {
            if RESERVED_PROOF_HEADERS.contains(&header.name.as_slice()) {
                return Err(IngressReject::ReservedProofHeader);
            }
            if HOP_BY_HOP_HEADERS.contains(&header.name.as_slice()) {
                return Err(IngressReject::HeaderMalformed);
            }
            if INBOUND_AUTH_HEADERS.contains(&header.name.as_slice()) {
                inbound_auth_count += 1;
            }
            if header.name == b"content-length" {
                if content_length.is_some() {
                    return Err(IngressReject::HeaderMalformed);
                }
                content_length = Some(parse_content_length(&header.value)?);
            }
            if header.name == b"content-type" {
                content_type_count += 1;
            }
        }

        if inbound_auth_count > 1 {
            return Err(IngressReject::ConflictingInboundAuthentication);
        }
        if content_type_count > 1 {
            return Err(IngressReject::HeaderMalformed);
        }
        if content_length.is_some_and(|declared| declared != body.len()) {
            return Err(IngressReject::RequestMalformed);
        }

        Ok(Self {
            method,
            target: target.to_vec(),
            headers,
            body,
        })
    }

    /// 返回方法。
    pub const fn method(&self) -> HttpMethod {
        self.method
    }

    /// 返回经过严格 origin-form 校验、未二次解码的 target。
    pub fn target(&self) -> &[u8] {
        &self.target
    }

    /// 返回原始正文。
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub(crate) fn content_type(&self) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|header| header.name == b"content-type")
            .map(|header| header.value.as_slice())
    }

    pub(crate) fn path(&self) -> &[u8] {
        self.target
            .splitn(2, |byte| *byte == b'?')
            .next()
            .unwrap_or_default()
    }

    pub(crate) fn has_query(&self) -> bool {
        self.target.contains(&b'?')
    }

    pub(crate) fn into_sanitized(mut self, semantic_headers: &[Vec<u8>]) -> SanitizedRequest {
        let headers = std::mem::take(&mut self.headers)
            .into_iter()
            .filter(|header| {
                !INBOUND_AUTH_HEADERS.contains(&header.name.as_slice())
                    && !STRIPPED_SENSITIVE_HEADERS.contains(&header.name.as_slice())
                    && header.name != b"content-length"
                    && (header.name == b"content-type"
                        || semantic_headers
                            .iter()
                            .any(|allowed| allowed.as_slice() == header.name))
            })
            .collect();

        SanitizedRequest {
            method: self.method,
            target: std::mem::take(&mut self.target),
            headers,
            body: std::mem::take(&mut self.body),
        }
    }
}

impl Drop for RawIngressRequest {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}

pub(crate) struct SanitizedRequest {
    pub(crate) method: HttpMethod,
    pub(crate) target: Vec<u8>,
    pub(crate) headers: Vec<RawHeader>,
    pub(crate) body: Vec<u8>,
}

impl Drop for SanitizedRequest {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn parse_content_length(value: &[u8]) -> Result<usize, IngressReject> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return Err(IngressReject::HeaderMalformed);
    }
    let text = std::str::from_utf8(value).map_err(|_| IngressReject::HeaderMalformed)?;
    text.parse::<usize>()
        .map_err(|_| IngressReject::HeaderMalformed)
}

fn validate_origin_form(target: &[u8]) -> Result<(), IngressReject> {
    if target.is_empty()
        || target.len() > MAX_REQUEST_TARGET_BYTES
        || target[0] != b'/'
        || target.contains(&b'#')
        || target.contains(&b'\\')
        || target
            .iter()
            .any(|byte| !byte.is_ascii() || byte.is_ascii_control() || *byte == b' ')
    {
        return Err(IngressReject::RequestMalformed);
    }

    let path = target
        .splitn(2, |byte| *byte == b'?')
        .next()
        .unwrap_or_default();
    for segment in path.split(|byte| *byte == b'/') {
        if segment == b"." || segment == b".." {
            return Err(IngressReject::RequestMalformed);
        }
    }

    let lower = target
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if lower
        .windows(3)
        .any(|window| matches!(window, b"%2f" | b"%5c" | b"%2e" | b"%25" | b"%00"))
    {
        return Err(IngressReject::RequestMalformed);
    }

    Ok(())
}
