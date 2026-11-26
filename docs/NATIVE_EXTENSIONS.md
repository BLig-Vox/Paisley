# Paisley native page scripts (experimental)

The CEF build can load locally installed `index.js` and an optional `style.css` from a profile's `native-extensions/<exact-host>/<name>/` directory. For example, install the YouTube demo by copying `examples/native-extensions/youtube-clean` to `<profile-data>/native-extensions/www.youtube.com/youtube-clean`. Reload the page after installing or editing it. Different subdomains need their own host directory. Scripts do not run in private windows.

Scripts run in the page's JavaScript context with access to that page and its data. Install only code you trust. This is not an isolated permission system, and there is no extension manager UI, background runtime, Chrome API, or automatic update mechanism yet. The example's download control can only request an exposed direct HTTP(S) media URL; YouTube commonly supplies blob/segmented streams, which this prototype cannot download. It does not bypass DRM.

The files must be plain browser-compatible JavaScript and CSS. Bun can be used separately to bundle dependencies into `index.js`; Bun and Node APIs are not supplied in the page.
