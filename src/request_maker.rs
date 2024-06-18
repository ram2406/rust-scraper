use derive_more::Display;
use reqwest::{header::{HeaderName, HeaderValue}, RequestBuilder};
use tokio_retry::{strategy::ExponentialBackoff, Retry};
use tracing::warn;
use tracing_subscriber::field::display;
use std::{any::TypeId, collections::HashMap, future::Future, iter::Take, str::FromStr, time::Duration};
use thiserror::Error;

type HeadersMap = HashMap<String, String>;
type StatusVec = Vec<u16>;

#[derive(Default)]
pub struct RequestParams {
    pub method: String,
    pub url: String,

    pub headers: HeadersMap,
    pub status_forcelist: Option<StatusVec>,
}

#[derive(Debug)]
pub struct RequestMakerConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub backoff_factor: u32,

    pub headers: HeadersMap,
    pub status_forcelist: StatusVec,
}

#[derive(Debug)]
pub struct RequestMaker {
    client: reqwest::Client,
    pub config: RequestMakerConfig,
    retry_strategy:  Take<ExponentialBackoff>,
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

#[derive(derive_more::Error, Debug, Display)]
#[display(fmt="status [{status_code:?}] from forcelist [{status_forcelist:?}]")]
pub struct StatusCodeError {
    status_code: reqwest::StatusCode,
    status_forcelist: StatusVec,
}

#[derive(Error, Debug, Display)]
pub enum RequestMakerError {
    ReqwestError(#[from] reqwest::Error),
    StatusCodeError(#[from] StatusCodeError),
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

pub fn url_combine(url: &str, urlp: &str) -> String {
    if urlp.is_empty() || urlp == "/" {
        return url.to_owned();
    }
    let up: Vec<&str> = url.splitn(2, '?').collect();
    let left = up[0].trim_end_matches('/');
    let (is_multi, has_prefix) = (up.len() > 1, urlp.chars().nth(0).unwrap() == '/');

    if !is_multi && has_prefix {
        return format!("{left}{urlp}");
    }

    let up_second = if up.len() > 1 { up[1] } else { "" };
    if is_multi && has_prefix {
        return format!("{left}{urlp}?{up_second}");
    }
    
    let handled_urlp = urlp.trim_matches(|c| c == '?' || c == '&');
    if is_multi && ! has_prefix {
        return format!("{left}?{handled_urlp}&{up_second}");
    }
    // if ! is_multi && ! has_prefix
	return format!("{left}?{handled_urlp}")
}


pub fn url_combine_all(urls: &[&str]) -> String {
	let mut url = urls[0].to_owned();
	for idx in 1..urls.len() {
		url = url_combine(&url, urls[idx]);
	}
	return url;
}


#[allow(useless_deprecated)]
#[deprecated(note="complex")]
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
        fn handle_request(req: &mut reqwest_middleware::RequestBuilder, status_forcelist: &Option<StatusVec>) {
            if let Some(sfl) = &status_forcelist {
                req.extensions()
                    .insert(StatusVecWrapper { value: sfl.clone() });
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

        let retry_strategy = ExponentialBackoff::from_millis(1)
            .factor(config.backoff_factor.into())
            .take(config.max_retries.try_into().expect("unexpected max_retries value"));

        Ok(Self { client, config, retry_strategy })
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

        let status_forcelist_ref: &StatusVec =  if let Some(ref svec) = params.status_forcelist { svec }
                                                else { &self.config.status_forcelist };
        let resp = Retry::spawn(self.retry_strategy.clone(),  || async {
            
            let status_forcelist = status_forcelist_ref.clone();
            let req = req.try_clone().expect("somthing wrong with request object");
            let future = req.send();
            match future.await {
                Ok(resp) if status_forcelist.contains(&resp.status().as_u16()) => {
                    Err(RequestMakerError::StatusCodeError(
                        StatusCodeError { status_code: resp.status(), status_forcelist }
                    ))
                },
                e => e.map_err(|err| RequestMakerError::ReqwestError(err)),
            }.map_err(|err| {
                warn!("attempt with [{:?}]", err);
                err
            })

        }).await;
        
        resp
    }

    pub async fn request_text(
        &self,
        params: &RequestParams,
    ) -> Result<String, RequestMakerError> {
        Ok(self.request(params).await?.text().await?)
    }

}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;
    use tracing::{info, Level};

    use super::*;

    #[tokio::test]
    async fn test_request() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .try_init();


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

    #[tokio::test]
    async fn tokio_retry_test() {
        use tokio_retry::Retry;
        use tokio_retry::strategy::{ExponentialBackoff, jitter};

        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .try_init();
        
        
        
        async fn action() -> Result<u64, ()> {
            // do some real-world stuff here...
            info!("retry");
            Err(())
        }
        
        let retry_strategy = ExponentialBackoff::from_millis(10)
            .map(|d| d.mul_f64(333.0)) // add jitter to delays
            .take(3);
            // .collect::<Vec<Duration>>();    // limit to 3 retries
        
        
        let result = Retry::spawn(retry_strategy, action).await;
    }

}
