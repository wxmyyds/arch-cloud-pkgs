use std::sync::LazyLock;

#[derive(Clone, Copy)]
enum Language {
    English,
    SimplifiedChinese,
}

#[derive(Clone, Copy)]
pub enum Key {
    CatalogParseFailed,
    CatalogVersionUnsupported,
    CatalogEmpty,
    CatalogInvalidPackageName,
    PackageNotFound,
    GhcrTokenFailed,
    GhcrRepeatedPagination,
    GhcrTagsFailed,
    GhcrUnexpectedPagination,
    GhcrNoVersions,
    GhcrVersionNotFound,
    OciManifestFailed,
    OciPkginfoMissing,
    PkgNameMissing,
    PkgVersionMissing,
    PkgArchitectureMissing,
    PkgNameMismatch,
    PkgVersionMismatch,
    PkgArchitectureMismatch,
    OciLayerCount,
    OciLayerType,
    DownloadDigestMalformed,
    DownloadFileCreateFailed,
    DownloadBlobReadFailed,
    DownloadSizeMismatch,
    DownloadDigestMismatch,
    VersionEmpty,
    VersionTagTooLong,
    DigestUnsupported,
    DigestMalformed,
    VercmpRunFailed,
    VercmpCompareFailed,
    VercmpUnexpectedOutput,
    SearchShortUsage,
    SearchLongUsage,
    SearchPatternInvalid,
    SearchNoMatches,
    SearchUnavailable,
    SearchDescription,
    InfoUsage,
    OperationMissing,
    InvalidUtf8PackageName,
    OptionValueMissing,
    InstallUsage,
    InstallVersionEmpty,
    InstallTargetInvalid,
    LocalOperationUnsupported,
    CloudOperationUnsupported,
    HelpTitle,
    HelpOperations,
    HelpList,
    HelpInfo,
    HelpSearch,
    HelpCheck,
    HelpInstall,
    HelpUpgrade,
    HelpLocalOperations,
    VersionTitle,
    ListHeader,
    ListUnavailable,
    InfoName,
    InfoVersion,
    InfoArchitecture,
    InfoDescription,
    InfoLayer,
    UpdateAvailable,
    UpdateCurrent,
    UpdateLocalNewer,
    UpdateNone,
    DependencyCycle,
    ConfirmTitle,
    ConfirmPrompt,
    ConfirmCancelled,
    TransactionHeader,
    TransactionDownloadSize,
    InstallAlreadyCurrent,
    TransactionNoPackages,
    TransactionFailed,
    ErrorPrefix,
}

static LANGUAGE: LazyLock<Language> = LazyLock::new(Language::from_environment);

impl Language {
    fn from_environment() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|locale| !locale.is_empty())
            .unwrap_or_default();
        Self::from_locale(&locale)
    }

    fn from_locale(locale: &str) -> Self {
        let language = locale.split(['.', '@']).next().unwrap_or("");
        let language = language.split(['_', '-']).next().unwrap_or("");
        if language.eq_ignore_ascii_case("zh") || language.eq_ignore_ascii_case("chinese") {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }
}

fn english(key: Key) -> &'static str {
    use Key::*;
    match key {
        CatalogParseFailed => "Failed to parse cloud package catalog",
        CatalogVersionUnsupported => "Unsupported package catalog version: {version}",
        CatalogEmpty => "Package catalog is empty",
        CatalogInvalidPackageName => "Invalid package name in catalog: {name}",
        PackageNotFound => "Package '{name}' is not in the cpkg cloud catalog",
        GhcrTokenFailed => "Failed to decode GHCR anonymous token",
        GhcrRepeatedPagination => "GHCR returned a repeated tag pagination link",
        GhcrTagsFailed => "Failed to decode GHCR tag list",
        GhcrUnexpectedPagination => "GHCR returned an unexpected pagination URL",
        GhcrNoVersions => "No published package versions found on GHCR",
        GhcrVersionNotFound => "Version {version} was not found on GHCR",
        OciManifestFailed => "Failed to decode OCI manifest",
        OciPkginfoMissing => "OCI manifest is missing .PKGINFO metadata",
        PkgNameMissing => ".PKGINFO is missing the pkgname field",
        PkgVersionMissing => ".PKGINFO is missing the pkgver field",
        PkgArchitectureMissing => ".PKGINFO is missing the arch field",
        PkgNameMismatch => "Package name mismatch: expected {expected}, got {actual}",
        PkgVersionMismatch => "Package version mismatch: OCI tag is {tag}, .PKGINFO says {metadata}",
        PkgArchitectureMismatch => "Package architecture {architecture} does not match this system",
        OciLayerCount => "Expected one pacman package layer in the OCI manifest",
        OciLayerType => "Unsupported OCI package layer media type",
        DownloadDigestMalformed => "Malformed package digest",
        DownloadFileCreateFailed => "Failed to create package download file",
        DownloadBlobReadFailed => "Failed to read package blob",
        DownloadSizeMismatch => "Package size mismatch: expected {expected} bytes, got {actual}",
        DownloadDigestMismatch => "Package SHA-256 digest mismatch",
        VersionEmpty => "Package version cannot be empty",
        VersionTagTooLong => "Encoded OCI tag exceeds the OCI tag length limit",
        DigestUnsupported => "Only SHA-256 OCI digests are supported",
        DigestMalformed => "Malformed SHA-256 digest",
        VercmpRunFailed => "Failed to run vercmp (install pacman)",
        VercmpCompareFailed => "vercmp rejected a version comparison",
        VercmpUnexpectedOutput => "Unexpected vercmp output: {value}",
        SearchShortUsage => "Usage: cpkg -Ss <pattern>",
        SearchLongUsage => "Usage: cpkg --sync --search <pattern>",
        SearchPatternInvalid => "Invalid package search pattern",
        SearchNoMatches => "No cloud packages found matching '{query}'.",
        SearchUnavailable => "unavailable",
        SearchDescription => "Description",
        InfoUsage => "Usage: cpkg -Si <package>",
        OperationMissing => "Missing pacman operation",
        InvalidUtf8PackageName => "Package names must be valid UTF-8",
        OptionValueMissing => "Missing value for pacman option",
        InstallUsage => "Usage: cpkg -S <package[=version]...>",
        InstallVersionEmpty => "Package version cannot be empty: {target}",
        InstallTargetInvalid => "Invalid package target: {target}",
        LocalOperationUnsupported => "cpkg only handles cloud package operations; use pacman directly for removal and other local operations",
        CloudOperationUnsupported => "Only pacman-style cloud operations are supported; see 'cpkg --help'",
        HelpTitle => "cpkg {version} - GHCR cloud package client",
        HelpOperations => "Pacman-style cloud operations:",
        HelpList => "  cpkg -Sl                         List cloud packages",
        HelpInfo => "  cpkg -Si <package>               Show package information",
        HelpSearch => "  cpkg -Ss <pattern>               Search package names and descriptions",
        HelpCheck => "  cpkg -Qu [package]               Check cloud package updates",
        HelpInstall => "  cpkg -S <package[=version]...>  Install cloud packages",
        HelpUpgrade => "  cpkg -Syu [options]              Upgrade installed cloud packages",
        HelpLocalOperations => "For removal and other local package operations, run pacman directly.",
        VersionTitle => "cpkg {version}",
        ListHeader => "Name                 Version                  Description",
        ListUnavailable => "{name}: unavailable ({error})",
        InfoName => "Name: {name}",
        InfoVersion => "Version: {version}",
        InfoArchitecture => "Architecture: {architecture}",
        InfoDescription => "Description: {description}",
        InfoLayer => "OCI layer: {size} bytes, {digest}",
        UpdateAvailable => "{name}: {current} -> {latest}",
        UpdateCurrent => "{name}: up to date ({version})",
        UpdateLocalNewer => "{name}: local {current} is newer than cloud {latest} (no downgrade)",
        UpdateNone => "No cloud package updates available.",
        DependencyCycle => "Dependency cycle involving {package}",
        ConfirmTitle => "Proceed with installation?",
        ConfirmPrompt => " [Y/n] ",
        ConfirmCancelled => "Cancelled.",
        TransactionHeader => "Packages ({count})",
        TransactionDownloadSize => "Total Download Size: {size} MiB",
        InstallAlreadyCurrent => "{name} is already at {current}; cloud version is {latest}. No downgrade or reinstall.",
        TransactionNoPackages => "No packages selected",
        TransactionFailed => "pacman installation failed",
        ErrorPrefix => "cpkg",
    }
}

fn simplified_chinese(key: Key) -> &'static str {
    use Key::*;
    match key {
        CatalogParseFailed => "解析云端包清单失败",
        CatalogVersionUnsupported => "不支持的包清单版本：{version}",
        CatalogEmpty => "包清单为空",
        CatalogInvalidPackageName => "包清单中包含无效包名：{name}",
        PackageNotFound => "包“{name}”不在 cpkg 云端包清单中",
        GhcrTokenFailed => "解析 GHCR 匿名访问令牌失败",
        GhcrRepeatedPagination => "GHCR 返回了重复的标签分页链接",
        GhcrTagsFailed => "解析 GHCR 标签列表失败",
        GhcrUnexpectedPagination => "GHCR 返回了非预期的分页地址",
        GhcrNoVersions => "GHCR 中没有已发布的包版本",
        GhcrVersionNotFound => "GHCR 中未找到版本 {version}",
        OciManifestFailed => "解析 OCI 清单失败",
        OciPkginfoMissing => "OCI 清单缺少 .PKGINFO 元数据",
        PkgNameMissing => ".PKGINFO 缺少 pkgname 字段",
        PkgVersionMissing => ".PKGINFO 缺少 pkgver 字段",
        PkgArchitectureMissing => ".PKGINFO 缺少 arch 字段",
        PkgNameMismatch => "包名不匹配：预期 {expected}，实际为 {actual}",
        PkgVersionMismatch => "包版本不匹配：OCI 标签为 {tag}，.PKGINFO 中为 {metadata}",
        PkgArchitectureMismatch => "包架构 {architecture} 与当前系统不兼容",
        OciLayerCount => "OCI 清单应仅包含一个 pacman 包层",
        OciLayerType => "OCI 包层媒体类型不受支持",
        DownloadDigestMalformed => "包摘要格式无效",
        DownloadFileCreateFailed => "创建包下载文件失败",
        DownloadBlobReadFailed => "读取包数据失败",
        DownloadSizeMismatch => "包大小校验失败：预期 {expected} 字节，实际 {actual} 字节",
        DownloadDigestMismatch => "包 SHA-256 摘要校验失败",
        VersionEmpty => "包版本不能为空",
        VersionTagTooLong => "编码后的 OCI 标签超过长度限制",
        DigestUnsupported => "仅支持 SHA-256 OCI 摘要",
        DigestMalformed => "SHA-256 摘要格式无效",
        VercmpRunFailed => "运行 vercmp 失败（请安装 pacman）",
        VercmpCompareFailed => "vercmp 无法比较版本",
        VercmpUnexpectedOutput => "vercmp 返回了意外的比较结果：{value}",
        SearchShortUsage => "用法：cpkg -Ss <pattern>",
        SearchLongUsage => "用法：cpkg --sync --search <pattern>",
        SearchPatternInvalid => "包搜索正则表达式无效",
        SearchNoMatches => "未找到匹配的云端包：{query}",
        SearchUnavailable => "暂不可用",
        SearchDescription => "简介",
        InfoUsage => "用法：cpkg -Si <package>",
        OperationMissing => "缺少 pacman 操作选项",
        InvalidUtf8PackageName => "包名必须是有效 UTF-8",
        OptionValueMissing => "缺少 pacman 选项参数",
        InstallUsage => "用法：cpkg -S <package[=version]...>",
        InstallVersionEmpty => "包版本不能为空：{target}",
        InstallTargetInvalid => "包目标格式无效：{target}",
        LocalOperationUnsupported => "cpkg 只处理云端包操作；移除和其他本地包操作请直接使用 pacman",
        CloudOperationUnsupported => "仅支持 pacman 格式的云端包操作；请查看 cpkg --help",
        HelpTitle => "cpkg {version} - GHCR 云端包客户端",
        HelpOperations => "Pacman 格式的云端包操作：",
        HelpList => "  cpkg -Sl                         列出云端包",
        HelpInfo => "  cpkg -Si <package>               查看包信息",
        HelpSearch => "  cpkg -Ss <pattern>               搜索包名和简介",
        HelpCheck => "  cpkg -Qu [package]               检查云端包更新",
        HelpInstall => "  cpkg -S <package[=version]...>  安装云端包",
        HelpUpgrade => "  cpkg -Syu [options]              升级已安装的云端包",
        HelpLocalOperations => "卸载和其他本地包操作请直接使用 pacman。",
        VersionTitle => "cpkg {version}",
        ListHeader => "包名                 版本                     简介",
        ListUnavailable => "{name}：暂不可用（{error}）",
        InfoName => "包名：{name}",
        InfoVersion => "版本：{version}",
        InfoArchitecture => "架构：{architecture}",
        InfoDescription => "简介：{description}",
        InfoLayer => "OCI 包层：{size} 字节，摘要 {digest}",
        UpdateAvailable => "{name}：{current} -> {latest}",
        UpdateCurrent => "{name}：已是最新版本（{version}）",
        UpdateLocalNewer => "{name}：本地版本 {current} 高于云端版本 {latest}，不执行降级",
        UpdateNone => "没有可用的云端包更新。",
        DependencyCycle => "检测到依赖循环：{package}",
        ConfirmTitle => "进行安装吗？",
        ConfirmPrompt => " [Y/n] ",
        ConfirmCancelled => "已取消。",
        TransactionHeader => "软件包（{count}）",
        TransactionDownloadSize => "总下载大小：{size} MiB",
        InstallAlreadyCurrent => "{name} 已是 {current}；云端版本为 {latest}，不执行降级或重装。",
        TransactionNoPackages => "没有选中的包",
        TransactionFailed => "pacman 安装事务失败",
        ErrorPrefix => "cpkg 错误",
    }
}

pub fn text(key: Key) -> &'static str {
    match *LANGUAGE {
        Language::English => english(key),
        Language::SimplifiedChinese => simplified_chinese(key),
    }
}

pub fn format(key: Key, arguments: &[(&str, String)]) -> String {
    let mut message = text(key).to_owned();
    for (name, value) in arguments {
        message = message.replace(&format!("{{{name}}}"), value);
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_language_from_locale_identifier() {
        assert!(matches!(Language::from_locale("zh_CN.UTF-8"), Language::SimplifiedChinese));
        assert!(matches!(Language::from_locale("zh-Hans-CN"), Language::SimplifiedChinese));
        assert!(matches!(Language::from_locale("en_US.UTF-8"), Language::English));
        assert!(matches!(Language::from_locale("C.UTF-8"), Language::English));
    }

    #[test]
    fn formats_named_message_parameters() {
        let message = format(Key::SearchNoMatches, &[("query", "clavis".to_owned())]);
        assert!(message.contains("clavis"));
    }
}
