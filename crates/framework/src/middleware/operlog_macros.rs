//! `operlog!` macro — shorthand for `OperlogMarkLayer::new(title, business_type)`.

/// Route-level layer that marks a handler for operation logging.
///
/// The global `operlog::global_operlog` middleware (applied in main.rs)
/// reads this mark and writes the log after handler execution.
///
/// ```ignore
/// use framework::operlog;
/// use framework::middleware::operlog::BusinessType;
///
/// // In router():
/// .routes(routes!(create)
///     .layer(require_permission!("system:role:add"))
///     .layer(operlog!("角色管理", Insert)))
///
/// // BusinessType variants: Other, Insert, Update, Delete, Grant, Export, Import, Clean
/// ```
#[macro_export]
macro_rules! operlog {
    ($title:expr, $biz_type:ident) => {
        $crate::middleware::operlog::OperlogMarkLayer::new(
            $title,
            $crate::middleware::operlog::BusinessType::$biz_type,
        )
    };
}
