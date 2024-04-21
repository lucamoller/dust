
#[cfg(feature = "ssr")]
pub async fn serve<IV>(
    app_fn: impl Fn() -> IV + 'static + Clone + Send,
) where
IV: leptos::IntoView + 'static,
{
    use leptos_axum::LeptosRoutes;

    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = leptos::get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = leptos_axum::generate_route_list(app_fn.clone());

    // build our application with a route
    let app = axum::Router::new()
        .leptos_routes(&leptos_options, routes, app_fn)
        .fallback(crate::file_handler::file_handler)
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    leptos::logging::log!("listening on http://{}", &addr);
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
