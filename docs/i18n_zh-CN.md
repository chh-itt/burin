# 国际化 (i18n)

需要 `i18n` feature（Fluent）。

## 设置

```toml
[dependencies]
burin = { git = "...", features = ["i18n"] }
```

将 `.ftl` 文件放入 `locales/`：

```
locales/
  en-US/
    main.ftl
  zh-CN/
    main.ftl
```

```
# main.ftl
hello = 你好，{$name}！欢迎回来。您有 {$count} 条新消息。
```

## 使用

```rust
use burin::t;

// t! 宏: 从 Fluent bundle 读取
let greeting = t!(ctx, "hello", name = "Alice");       // Signal<String>
let welcome = t!(ctx, "welcome-back", count = 5);        // Signal<String>

// 绑定到 Text widget
Text::new("").bind(greeting);

// 运行时切换语言
ctx.set_locale("zh-CN");
```
