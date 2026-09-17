fn main() {
    fn requires_menu<B: rsvz_backend_api::MainMenuBackend>() {}
    requires_menu::<rsvz::__private::CurrentBackend>();
}
