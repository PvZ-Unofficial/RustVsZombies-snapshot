include!("support.rs");

mod user_script {
    use rsvz_macros::script;

    #[script]
    fn script() {}
}

fn main() {
    let _ = user_script::script();
}
