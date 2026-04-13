//! SMS client abstraction — trait + factory + mock implementations.

use async_trait::async_trait;
use std::collections::HashMap;

use framework::error::AppError;
use framework::response::ResponseCode;

#[derive(Debug, Clone)]
pub struct SmsSendParams {
    pub mobile: String,
    pub signature: String,
    pub api_template_id: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug)]
pub struct SmsSendResult {
    pub success: bool,
    pub api_send_code: Option<String>,
    pub error_msg: Option<String>,
}

#[async_trait]
pub trait SmsClient: Send + Sync {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult;
}

/// Create an SMS client by channel code.
pub fn create_client(
    channel_code: &str,
    _api_key: &str,
    _api_secret: &str,
) -> Result<Box<dyn SmsClient>, AppError> {
    match channel_code {
        "aliyun" => Ok(Box::new(AliyunSmsClient)),
        "tencent" => Ok(Box::new(TencentSmsClient)),
        "huawei" => Ok(Box::new(HuaweiSmsClient)),
        _ => Err(AppError::business(ResponseCode::SMS_CHANNEL_NOT_SUPPORTED)),
    }
}

// ---------------------------------------------------------------------------
// Mock implementations
// ---------------------------------------------------------------------------

struct AliyunSmsClient;

#[async_trait]
impl SmsClient for AliyunSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        tracing::info!(mobile = %params.mobile, provider = "aliyun", "mock sms send");
        SmsSendResult {
            success: true,
            api_send_code: Some(format!("aliyun-mock-{}", uuid::Uuid::new_v4())),
            error_msg: None,
        }
    }
}

struct TencentSmsClient;

#[async_trait]
impl SmsClient for TencentSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        tracing::info!(mobile = %params.mobile, provider = "tencent", "mock sms send");
        SmsSendResult {
            success: true,
            api_send_code: Some(format!("tencent-mock-{}", uuid::Uuid::new_v4())),
            error_msg: None,
        }
    }
}

struct HuaweiSmsClient;

#[async_trait]
impl SmsClient for HuaweiSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        tracing::info!(mobile = %params.mobile, provider = "huawei", "mock sms send");
        SmsSendResult {
            success: true,
            api_send_code: Some(format!("huawei-mock-{}", uuid::Uuid::new_v4())),
            error_msg: None,
        }
    }
}
