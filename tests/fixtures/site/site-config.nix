{
  site_name = "Niche Test Site";
  base_url = "https://example.test";
  language = "en";
  posts_per_page = 5;

  nav = [
    { label = "Home"; url = "/"; }
    { label = "Archive"; url = "/archive/"; }
    # external opt-out: exercises the validateNavItem fallthrough
    { label = "Source"; url = "https://example.test/source"; external = true; }
  ];

  feed = {
    enable = true;
    title = "Niche Test Site";
    description = "Engine smoke-test fixture.";
  };

  author = {
    name = "test";
    email = "test@example.test";
  };
}
