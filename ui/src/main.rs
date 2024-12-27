use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;
use gloo_net::http::Request;
use gloo_timers::callback::Interval;


static API_ENDPOINT: &str = match option_env!("API_ENDPOINT") {
    Some(val) => val,
    None => "https://localhost:3001",
};


#[function_component(App)]
fn app() -> Html {
    let info_text = use_state(|| String::from("{}"));

    let fetch_info = {
        let info_text = info_text.clone();
        Callback::from(move |_| {
            let info_text = info_text.clone();
            spawn_local(async move {
                let url = format!("{}/info", API_ENDPOINT);

                match Request::get(&url).send().await {
                    Ok(resp) => {
                        let text = resp.text().await.unwrap_or_else(|_| "{}".into());
                        info_text.set(text);
                    }
                    Err(err) => {
                        info_text.set(format!("{{\"error\": \"{}\"}}", err));
                    }
                }
            });
        })
    };

    {
        let fetch_info = fetch_info.clone();
        use_effect_with_deps(
            move |_| {
                fetch_info.emit(());
                let interval = Interval::new(1_000, move || {
                    fetch_info.emit(());
                });
                move || drop(interval)
            },
            (),
        );
    }

    html! {
        <div style="margin:2rem;font-family:Arial, sans-serif;">
            <p>
                <strong>{"Info: "}</strong>
                { &*info_text }
            </p>
        </div>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}
