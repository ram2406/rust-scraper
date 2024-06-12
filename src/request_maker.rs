use derive_more::Display;
use reqwest::header::{HeaderName, HeaderValue};
use std::{any::TypeId, collections::HashMap, str::FromStr, time::Duration};
use thiserror::Error;

type HeadersMap = HashMap<String, String>;
type StatusVec = Vec<u16>;

#[derive(Default)]
pub struct RequestParams {
    method: String,
    url: String,

    headers: HeadersMap,
    status_forcelist: Option<StatusVec>,
}

#[derive(Debug)]
pub struct RequestMakerConfig {
    timeout: Duration,
    max_retries: u32,
    backoff_factor: u32,

    headers: HeadersMap,
    status_forcelist: StatusVec,
}

pub struct RequestMaker {
    client: reqwest_middleware::ClientWithMiddleware,
    config: RequestMakerConfig,
}

impl Default for RequestMakerConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_retries: 3,
            backoff_factor: 2,
            headers: HeadersMap::from_iter([(
                "user-agent".into(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/113.0"
                    .into(),
            )]),
            status_forcelist: vec![500, 502, 503, 504],
        }
    }
}

#[derive(Error, Debug, Display)]
pub enum RequestMakerError {
    InitError(#[from] reqwest::Error),
    RequestRetryError(#[from] retry::Error<reqwest::Error>),
    InvalidMethod(#[from] http::method::InvalidMethod),
    MiddlewareError(#[from] reqwest_middleware::Error),
}

fn from(hashmap: &HeadersMap) -> reqwest::header::HeaderMap {
    hashmap
        .iter()
        .map(|(k, v)| {
            (
                HeaderName::from_str(k).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            )
        })
        .collect()
}

mod middleware {
    use async_trait::async_trait;
    use reqwest_middleware::*;
    use reqwest_retry::{policies::*, *};

    use super::{RequestMakerConfig, StatusVec};

    pub struct CheckStatusForelistMiddleware {
        pub status_forcelist: StatusVecWrapper,
    }
    #[derive(Clone, Debug)]
    pub struct StatusVecWrapper {
        pub value: StatusVec,
    }

    pub struct CheckStatusForelistStrategy;

    impl CheckStatusForelistStrategy {
        fn check_response(
            &self,
            response: &reqwest::Response,
            extensions: &http::Extensions,
        ) -> bool {
            let ext: Option<&StatusVecWrapper> = extensions.get();
            if let Some(wrapper) = ext {
                wrapper.value.contains(&response.status().as_u16())
            } else {
                false
            }
        }
    }

    impl RetryableStrategy for CheckStatusForelistStrategy {
        fn handle(&self, res: &Result<reqwest::Response>) -> Option<Retryable> {
            match res {
                // retry if in forcelist
                Ok(resp) if self.check_response(&resp, &resp.extensions()) => {
                    Some(Retryable::Transient)
                }
                // otherwise do not retry a successful request
                Ok(_) => None,
                // but maybe retry a request failure
                Err(error) => default_on_request_failure(error),
            }
        }
    }

    // #[async_trait]
    // #[allow(useless_deprecated)]
    // impl Middleware for CheckStatusForelistStrategy {
    //     #[deprecated(note="retry policy ignore Middleware errors and Reqwest errors are not available, need to use retry strategy")]
    //     async fn handle(
    //         &'_ self,
    //         req: reqwest::Request,
    //         extensions: &'_ mut http::Extensions,
    //         next: Next<'_>,
    //     ) -> Result<reqwest::Response> {
    //         let res = next.run(req, extensions).await;
    //         match res {
    //             Ok(resp) if self.check_response(&resp, extensions) =>
    //                 Err(Error::Middleware(anyhow!("status from forcelist [{}]", resp.status()))),
    //             _ => res,
    //         }
    //     }
    // }

    #[async_trait]
    impl Middleware for CheckStatusForelistMiddleware {
        async fn handle(
            &'_ self,
            req: reqwest::Request,
            extensions: &'_ mut http::Extensions,
            next: Next<'_>,
        ) -> Result<reqwest::Response> {
            let ext = extensions
                .get_or_insert(self.status_forcelist.clone())
                .to_owned();
            let rs = next.run(req, extensions).await;
            rs.map(|mut resp| {
                resp.extensions_mut().get_or_insert(ext);
                resp
            })
        }
    }

    pub fn client_with_middleware(
        client: reqwest::Client,
        config: &RequestMakerConfig,
    ) -> ClientWithMiddleware {
        let retry_policy = ExponentialBackoff::builder()
            .base(config.backoff_factor)
            .build_with_max_retries(config.max_retries);

        let retry_middleware = RetryTransientMiddleware::new_with_policy_and_strategy(
            retry_policy,
            CheckStatusForelistStrategy {},
        );

        let status_extend_middleware = CheckStatusForelistMiddleware {
            status_forcelist: StatusVecWrapper {
                value: config.status_forcelist.clone(),
            },
        };

        let client = ClientBuilder::new(client)
            .with(retry_middleware)
            .with(status_extend_middleware)
            .build();
        client
    }
}

impl RequestMaker {
    pub fn create(config: RequestMakerConfig) -> Result<Self, RequestMakerError> {
        let client = reqwest::Client::builder()
            .default_headers(from(&config.headers))
            .pool_idle_timeout(config.timeout)
            .timeout(config.timeout)
            .build()?;

        let client = middleware::client_with_middleware(client, &config);
        Ok(Self { client, config })
    }

    pub async fn request(
        &self,
        params: &RequestParams,
    ) -> Result<reqwest::Response, RequestMakerError> {
        let method = reqwest::Method::from_bytes(params.method.to_uppercase().as_bytes())?;
        let mut req = self.client.request(method, params.url.as_str());
        for (k, v) in params.headers.iter() {
            req = req.header(
                HeaderName::from_str(k).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        if let Some(sfl) = &params.status_forcelist {
            req.extensions()
                .insert(middleware::StatusVecWrapper { value: sfl.clone() });
        }

        let future = req.send();
        let resp = future.await?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::*;

    #[tokio::test]
    async fn test_request() {
        tracing_subscriber::fmt()
            .with_max_level(Level::WARN)
            .init();

        let maker = RequestMaker::create(RequestMakerConfig {
            status_forcelist: vec![200],
            max_retries: 2,
            backoff_factor: 3,
            ..RequestMakerConfig::default()
        })
        .expect("maker is broken");

        let res = maker
            .request(&RequestParams {
                url: "http://google.com".to_string(),
                method: "GET".to_string(),
                status_forcelist: Some(vec![]),
                ..RequestParams::default()
            })
            .await;
        println!("{:?}", res.map(|r| r.status()));

        let res = maker
        .request(&RequestParams {
            url: "http://google.com".to_string(),
            method: "GET".to_string(),
            // status_forcelist: Some(vec![]),
            ..RequestParams::default()
        })
        .await;

        println!("{:?}", res.map(|r| r.status()));
    }
}
