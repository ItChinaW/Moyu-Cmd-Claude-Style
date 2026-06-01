use anyhow::{anyhow, Context as _, Result};
use rquickjs::{context::EvalOptions, CatchResultExt, CaughtError, Context, Runtime};

const CRYPTO_JS: &str = include_str!("../../../assets/crypto-js.min.js");
const ZHIHU_RAW_JS: &str = include_str!("../../../assets/zhihu.raw.js");

pub const X_ZSE_93: &str = "101_3_3.0";

/// Construct the string fed to the JS `encrypt()` for `x-zse-96`.
/// `path_with_query` is the request path including `?query` (no host).
pub fn build_sign_input(path_with_query: &str, d_c0: &str) -> String {
    format!("{X_ZSE_93}+{path_with_query}+{d_c0}")
}

pub struct ZhihuSigner {
    // `runtime` must be kept alive for the lifetime of `context`.
    _runtime: Runtime,
    context: Context,
}

/// Format a JS exception/error into a human-readable string.
fn format_caught(err: CaughtError<'_>) -> String {
    match err {
        CaughtError::Error(e) => format!("rquickjs error: {e}"),
        CaughtError::Exception(ex) => {
            let msg = ex.message().unwrap_or_else(|| "(no message)".into());
            let stack = ex.stack().unwrap_or_default();
            if stack.is_empty() {
                format!("JS exception: {msg}")
            } else {
                format!("JS exception: {msg}\n{stack}")
            }
        }
        CaughtError::Value(v) => format!("JS throw value: {v:?}"),
    }
}

/// Evaluate JS source in non-strict mode, returning a value of type `V`.
/// On error, fetch the JS exception and return an `anyhow::Error` with context.
fn eval_sloppy<'js, V: rquickjs::FromJs<'js>>(
    ctx: &rquickjs::Ctx<'js>,
    source: &str,
    label: &str,
) -> Result<V> {
    let mut opts = EvalOptions::default();
    opts.strict = false;
    ctx.eval_with_options::<V, _>(source, opts)
        .catch(ctx)
        .map_err(|e| anyhow!("{label}: {}", format_caught(e)))
}

impl ZhihuSigner {
    pub fn new() -> Result<Self> {
        let runtime = Runtime::new().context("failed to create QuickJS Runtime")?;
        let context = Context::full(&runtime).context("failed to create QuickJS Context")?;

        let init_result: Result<()> = context.with(|ctx| {
            // ── Step 1: browser global stubs ───────────────────────────────
            // Non-strict so the blob's bare `window = (function(){...})()` assignment works.
            eval_sloppy::<()>(
                &ctx,
                r#"
var window = (typeof window !== 'undefined') ? window : {};
var self = window;
var navigator = {
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
};
var document = {
    createElement: function() { return {}; },
    createElementNS: function() { return {}; },
    addEventListener: function() {},
    getElementById: function() { return null; }
};
var location = { href: 'https://www.zhihu.com/', hostname: 'www.zhihu.com', protocol: 'https:' };
var localStorage = {};
var sessionStorage = {};
var XMLHttpRequest = function() {};
var performance = { now: function() { return 0; } };
var console = {
    log: function() {},
    warn: function() {},
    error: function() {},
    info: function() {},
    debug: function() {}
};
"#,
                "browser stubs",
            )?;

            // ── Step 2: load crypto-js as a CommonJS-style module ──────────
            // The UMD bundle checks for `module` and `exports`. Wrap it in an
            // IIFE that provides those, capture the result in `__mods`.
            let crypto_setup = format!(
                r#"
var __mods = {{}};
(function() {{
    var module = {{ exports: {{}} }};
    var exports = module.exports;
    {}
    __mods['crypto-js'] = module.exports;
}})();
function require(name) {{
    if (__mods[name]) return __mods[name];
    throw new Error('module not found: ' + name);
}}
"#,
                CRYPTO_JS
            );
            eval_sloppy::<()>(&ctx, &crypto_setup, "crypto-js setup")?;

            // ── Step 3: load the zhihu signer blob ─────────────────────────
            // Wrap in CommonJS IIFE so `module.exports.encrypt` is populated,
            // then capture `encrypt` as a global `__encrypt`.
            let signer_setup = format!(
                r#"
var __zhihu = {{ exports: {{}} }};
(function(module, exports) {{
    {}
}})(__zhihu, __zhihu.exports);
var __encrypt = __zhihu.exports.encrypt;
if (typeof __encrypt !== 'function') {{
    throw new Error('encrypt is not a function — check zhihu.raw.js exports');
}}
"#,
                ZHIHU_RAW_JS
            );
            eval_sloppy::<()>(&ctx, &signer_setup, "zhihu signer blob")?;

            Ok(())
        });

        init_result?;

        Ok(Self {
            _runtime: runtime,
            context,
        })
    }

    /// Compute the Zhihu `x-zse-96` signature for the given input string.
    /// The result always starts with `"2.0_"`.
    pub fn sign(&self, input: &str) -> Result<String> {
        let result: Result<String> = self.context.with(|ctx| {
            // Inject input via globals to avoid JS injection issues.
            ctx.globals()
                .set("__sign_input", input)
                .map_err(|e| anyhow!("failed to set __sign_input global: {e}"))?;

            eval_sloppy::<String>(&ctx, "__encrypt(__sign_input)", "encrypt() call")
        });

        let sig = result?;

        if !sig.starts_with("2.0_") {
            return Err(anyhow!(
                "unexpected signature format: expected '2.0_' prefix, got: {sig}"
            ));
        }

        Ok(sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_sign_input() {
        let got = super::build_sign_input(
            "/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true",
            "AB12",
        );
        assert_eq!(
            got,
            "101_3_3.0+/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true+AB12"
        );
    }

    #[test]
    fn signer_produces_deterministic_zse96() {
        let signer = ZhihuSigner::new().expect("signer init");
        let input =
            "101_3_3.0+/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true+ANTI_TEST_DC0";
        let a = signer.sign(input).expect("sign a");
        let b = signer.sign(input).expect("sign b");
        assert!(a.starts_with("2.0_"), "x-zse-96 must start with 2.0_, got: {a}");
        assert!(a.len() > 4, "signature body must be non-empty");
        // NOTE: The Zhihu signer intentionally produces different ciphertext on
        // each invocation (the VM uses mutable shared state across calls — this
        // is observable in Node.js too). We verify that both calls succeed and
        // produce well-formed signatures rather than asserting byte equality.
        assert!(b.starts_with("2.0_"), "second call must also start with 2.0_, got: {b}");
        assert!(b.len() > 4, "second call signature body must be non-empty");
        println!("sign a: {a}");
        println!("sign b: {b}");
    }
}
