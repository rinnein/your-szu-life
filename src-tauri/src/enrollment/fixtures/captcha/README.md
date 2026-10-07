These six synthetic 250×80 challenges are rendered from MiSans Regular for a repeatable development benchmark, not captured from an account or school session. They contain no authentication tokens or cookies.

Target sequences: 中国人民, 大学生活, 学习知识, 科学技术, 文化教育, 深圳大学. Bottom order is target indices [2, 0, 3, 1]. Expected target box order is therefore [1, 3, 0, 2], with boxes at x=10+58*j, y=30, width/height=28. The top prompt is intentionally outside x=82..132 to exercise the corrected crop.

Run `cargo test --manifest-path src-tauri/Cargo.toml captcha::tests::synthetic_benchmark -- --ignored --nocapture`. This reports full four-point accuracy, rejection rate, and elapsed time; synthetic results do not establish real school captcha accuracy.
