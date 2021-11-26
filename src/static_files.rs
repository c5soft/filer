use axum::{
    body::{Body, BoxBody},
    error_handling::HandleErrorExt,
    http::{
        header::{self, HeaderValue},
        Request, Response, StatusCode,
    },
    routing::service_method_routing,
};
use std::convert::Infallible;
use tower::{util::BoxCloneService,  ServiceExt};
use tower_http::{
    services::ServeDir,
    set_header::{SetRequestHeader, SetResponseHeader},
};

pub(crate) fn make_service(
    static_path: String,
    cache_age_in_minute: i32,
) -> BoxCloneService<Request<Body>, Response<BoxBody>, Infallible> {
    let inner = ServeDir::new(static_path)
        .precompressed_br()
        .precompressed_gzip()
        .handle_error(|e: std::io::Error| {
            Ok::<_, std::convert::Infallible>((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Unhandled internal error: {}", e),
            ))
        });
    //Feed br first for Chrome
    let inner =
        SetRequestHeader::overriding(inner, header::ACCEPT_ENCODING, |req: &Request<Body>| {
            let accpt_encoding = req.headers().get(header::ACCEPT_ENCODING).map(|x| {
                if *x == HeaderValue::from_static("gzip, deflate, br") {
                    HeaderValue::from_static("br, gzip, deflate")
                } else {
                    x.to_owned()
                }
            });
            accpt_encoding
        });
    let inner = SetResponseHeader::if_not_present(
        inner,
        header::CACHE_CONTROL,
        HeaderValue::from_str(&format!("max-age={}", cache_age_in_minute * 60)).unwrap(),
    );
    service_method_routing::get(inner).boxed_clone()
}
