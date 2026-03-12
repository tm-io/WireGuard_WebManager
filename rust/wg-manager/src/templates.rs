use minijinja::{context, Environment};

pub fn build_env() -> Environment<'static> {
    let mut env = Environment::new();

    // 既存の templates/*.html を Jinja2 互換でレンダリングする
    env.add_template("base.html", include_str!("../../../templates/base.html"))
        .expect("add base.html");
    env.add_template("login.html", include_str!("../../../templates/login.html"))
        .expect("add login.html");
    env.add_template("dashboard.html", include_str!("../../../templates/dashboard.html"))
        .expect("add dashboard.html");
    env.add_template("peers.html", include_str!("../../../templates/peers.html"))
        .expect("add peers.html");
    env.add_template("settings.html", include_str!("../../../templates/settings.html"))
        .expect("add settings.html");
    env.add_template("docs_list.html", include_str!("../../../templates/docs_list.html"))
        .expect("add docs_list.html");
    env.add_template("docs_view.html", include_str!("../../../templates/docs_view.html"))
        .expect("add docs_view.html");

    env
}

pub fn render_login(env: &Environment<'static>, error: Option<&str>) -> Result<String, String> {
    let tmpl = env.get_template("login.html").map_err(|e| e.to_string())?;
    tmpl.render(context! { error => error }).map_err(|e| e.to_string())
}

pub fn render_dashboard(env: &Environment<'static>) -> Result<String, String> {
    let tmpl = env.get_template("dashboard.html").map_err(|e| e.to_string())?;
    // base.html で active_page などが参照されても落ちないように最低限渡す
    tmpl.render(context! { active_page => "dashboard" })
        .map_err(|e| e.to_string())
}

pub fn render_peers(env: &Environment<'static>) -> Result<String, String> {
    let tmpl = env.get_template("peers.html").map_err(|e| e.to_string())?;
    tmpl.render(context! { active_page => "peers" })
        .map_err(|e| e.to_string())
}

pub fn render_settings(env: &Environment<'static>) -> Result<String, String> {
    let tmpl = env.get_template("settings.html").map_err(|e| e.to_string())?;
    tmpl.render(context! { active_page => "settings" })
        .map_err(|e| e.to_string())
}

pub fn render_docs_list(env: &Environment<'static>, entries: serde_json::Value) -> Result<String, String> {
    let tmpl = env.get_template("docs_list.html").map_err(|e| e.to_string())?;
    tmpl.render(context! { active_page => "docs", entries => entries })
        .map_err(|e| e.to_string())
}

pub fn render_docs_view(
    env: &Environment<'static>,
    title: &str,
    html_body: &str,
) -> Result<String, String> {
    let tmpl = env.get_template("docs_view.html").map_err(|e| e.to_string())?;
    tmpl.render(context! { active_page => "docs", title => title, html_body => html_body })
        .map_err(|e| e.to_string())
}

