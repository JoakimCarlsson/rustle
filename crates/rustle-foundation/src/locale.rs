//! Localization: the locale in effect, and string resources looked up by key for it.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use rustle_runtime::CompositionLocal;
use rustle_ui::{LOCAL_LAYOUT_DIRECTION, LayoutDirection};

/// Languages written right to left.
const RIGHT_TO_LEFT: [&str; 8] = ["ar", "he", "fa", "ur", "ps", "yi", "dv", "ug"];

/// Scripts written right to left, which decide the direction when a tag names one.
const RIGHT_TO_LEFT_SCRIPTS: [&str; 6] = ["Arab", "Hebr", "Thaa", "Syrc", "Nkoo", "Adlm"];

/// A language and, optionally, a region, written as a BCP 47 tag such as `en`, `pt-BR` or
/// `ar-EG`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale {
    tag: String,
}

impl Locale {
    /// A locale for `tag`. Underscores are read as hyphens, so `pt_BR` is `pt-BR`.
    pub fn new(tag: impl AsRef<str>) -> Self {
        Self {
            tag: tag.as_ref().replace('_', "-"),
        }
    }

    /// The locale the system is set to, read from `LC_ALL`, `LC_MESSAGES` and `LANG`, or
    /// `en` when none names a language.
    pub fn system() -> Self {
        static SYSTEM: OnceLock<Locale> = OnceLock::new();
        SYSTEM
            .get_or_init(|| {
                ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .filter_map(|name| std::env::var(name).ok())
                    .map(|value| value.split(['.', '@']).next().unwrap_or("").to_string())
                    .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
                    .map_or_else(|| Locale::new("en"), Locale::new)
            })
            .clone()
    }

    /// The whole tag, such as `pt-BR`.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The language part of the tag, such as `pt`.
    pub fn language(&self) -> &str {
        self.tag.split('-').next().unwrap_or("")
    }

    /// The region part of the tag, such as `BR` or the UN M.49 code `419`, if it has one.
    pub fn region(&self) -> Option<&str> {
        self.tag.split('-').skip(1).find(|part| {
            (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
        })
    }

    /// The script part of the tag, such as `Hans` or `Arab`, if it has one.
    pub fn script(&self) -> Option<&str> {
        self.tag
            .split('-')
            .skip(1)
            .find(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()))
    }

    /// The direction the locale is written in: its script's if the tag names one (`ur-Latn`
    /// is left to right, `az-Arab` right to left), otherwise its language's.
    pub fn layout_direction(&self) -> LayoutDirection {
        let right_to_left = match self.script() {
            Some(script) => RIGHT_TO_LEFT_SCRIPTS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(script)),
            None => RIGHT_TO_LEFT.contains(&self.language().to_ascii_lowercase().as_str()),
        };
        if right_to_left {
            LayoutDirection::Rtl
        } else {
            LayoutDirection::Ltr
        }
    }

    /// The tags to look a key up under, most specific first: the whole tag, then the tag with
    /// its last part dropped, and so on down to the language, all in lower case because tags do
    /// not tell letter cases apart.
    fn lookup_tags(&self) -> Vec<String> {
        let lower = self.tag.to_ascii_lowercase();
        let mut tags = vec![lower.clone()];
        let mut rest = lower.as_str();
        while let Some((shorter, _)) = rest.rsplit_once('-') {
            tags.push(shorter.to_string());
            rest = shorter;
        }
        tags
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::system()
    }
}

/// The locale in effect. Provide another with [`provide_locale`] to localize everything
/// below.
pub static LOCAL_LOCALE: CompositionLocal<Locale> = CompositionLocal::new(Locale::system);

/// Strings by key, for each locale.
///
/// A key is looked up in the exact locale, then in the locale with its last part dropped (so
/// `zh-Hans-CN` tries `zh-Hans` and then `zh`), then in the fallback locale the same way, which
/// is `en` unless [`with_fallback`](Self::with_fallback) says otherwise. Tags are matched
/// whatever their letter case.
#[derive(Debug, Clone)]
pub struct StringResources {
    locales: Rc<HashMap<String, HashMap<String, String>>>,
    fallback: Locale,
}

impl StringResources {
    /// Resources with no strings, falling back to `en`.
    pub fn new() -> Self {
        Self {
            locales: Rc::new(HashMap::new()),
            fallback: Locale::new("en"),
        }
    }

    /// Adds the `entries` (key and text pairs) of the locale with `tag`, replacing any it
    /// had.
    pub fn with_locale<'a>(
        mut self,
        tag: &str,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let table = entries
            .into_iter()
            .map(|(key, text)| (key.to_string(), text.to_string()))
            .collect();
        Rc::make_mut(&mut self.locales).insert(Locale::new(tag).tag.to_ascii_lowercase(), table);
        self
    }

    /// Sets the locale looked in last, when the one in effect lacks a key.
    pub fn with_fallback(mut self, tag: &str) -> Self {
        self.fallback = Locale::new(tag);
        self
    }

    /// The text of `key` for `locale`, if any locale in its lookup order has it.
    pub fn get(&self, locale: &Locale, key: &str) -> Option<&str> {
        locale
            .lookup_tags()
            .into_iter()
            .chain(self.fallback.lookup_tags())
            .find_map(|tag| self.locales.get(&tag)?.get(key))
            .map(String::as_str)
    }
}

impl Default for StringResources {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for StringResources {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.locales, &other.locales) && self.fallback == other.fallback
    }
}

/// The string resources in effect. Provide them with [`provide_string_resources`].
pub static LOCAL_STRING_RESOURCES: CompositionLocal<StringResources> =
    CompositionLocal::new(StringResources::new);

/// Runs `content` with `locale` in effect, and the layout direction its language is written
/// in, so a right-to-left locale mirrors rows and stacks below.
pub fn provide_locale<R>(locale: Locale, content: impl FnOnce() -> R) -> R {
    let direction = locale.layout_direction();
    LOCAL_LOCALE.provide(locale, || {
        LOCAL_LAYOUT_DIRECTION.provide(direction, content)
    })
}

/// Runs `content` with `resources` in effect for [`string_resource`].
pub fn provide_string_resources<R>(resources: StringResources, content: impl FnOnce() -> R) -> R {
    LOCAL_STRING_RESOURCES.provide(resources, content)
}

/// The text of `key` in the current [`LOCAL_LOCALE`], from the current
/// [`LOCAL_STRING_RESOURCES`]. A key nothing defines is returned as it is, so a missing
/// translation shows up on screen instead of blanking it.
pub fn string_resource(key: &str) -> String {
    string_resource_with(key, &[])
}

/// Like [`string_resource`], with `{0}`, `{1}` and so on in the text replaced by `args`.
pub fn string_resource_with(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let locale = LOCAL_LOCALE.current();
    let resources = LOCAL_STRING_RESOURCES.current();
    let text = resources.get(&locale, key).unwrap_or(key);
    fill_placeholders(text, args)
}

/// `text` with each `{n}` replaced by the `n`th of `args` in one pass, so a replacement that
/// itself contains `{1}` is left as it is. A placeholder with no such argument stays.
fn fill_placeholders(text: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut filled = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        filled.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let argument = after
            .split_once('}')
            .and_then(|(digits, _)| digits.parse::<usize>().ok())
            .and_then(|position| args.get(position));
        match argument {
            Some(argument) => {
                filled.push_str(&argument.to_string());
                rest = &after[after.find('}').map_or(0, |close| close + 1)..];
            }
            None => {
                filled.push('{');
                rest = after;
            }
        }
    }
    filled.push_str(rest);
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_falls_back_from_region_to_language_to_fallback() {
        let resources = StringResources::new()
            .with_locale("en", [("hi", "Hello"), ("bye", "Bye")])
            .with_locale("pt", [("hi", "Olá")])
            .with_locale("pt-BR", [("bye", "Tchau")]);
        let brazil = Locale::new("pt_BR");
        assert_eq!(resources.get(&brazil, "bye"), Some("Tchau"));
        assert_eq!(resources.get(&brazil, "hi"), Some("Olá"));
        assert_eq!(resources.get(&Locale::new("de"), "hi"), Some("Hello"));
        assert_eq!(resources.get(&brazil, "nope"), None);
    }

    #[test]
    fn lookup_drops_one_part_at_a_time() {
        let resources = StringResources::new()
            .with_locale("zh", [("a", "zh"), ("b", "zh")])
            .with_locale("zh-Hans", [("a", "hans")]);
        let locale = Locale::new("zh-Hans-CN");
        assert_eq!(resources.get(&locale, "a"), Some("hans"));
        assert_eq!(resources.get(&locale, "b"), Some("zh"));
    }

    #[test]
    fn lookup_ignores_letter_case_in_tags() {
        let resources = StringResources::new().with_locale("pt-br", [("hi", "Oi")]);
        assert_eq!(resources.get(&Locale::new("pt-BR"), "hi"), Some("Oi"));
        let upper = StringResources::new().with_locale("PT-BR", [("hi", "Oi")]);
        assert_eq!(upper.get(&Locale::new("pt-br"), "hi"), Some("Oi"));
    }

    #[test]
    fn the_fallback_is_used_last_and_can_be_changed() {
        let resources = StringResources::new()
            .with_locale("en", [("hi", "Hello")])
            .with_locale("fr", [("hi", "Salut"), ("only", "seulement")])
            .with_fallback("fr");
        assert_eq!(resources.get(&Locale::new("de"), "hi"), Some("Salut"));
        assert_eq!(resources.get(&Locale::new("en-GB"), "hi"), Some("Hello"));
        assert_eq!(resources.get(&Locale::new("en"), "only"), Some("seulement"));
    }

    #[test]
    fn a_fallback_with_a_region_falls_back_to_its_language() {
        let resources = StringResources::new()
            .with_locale("pt", [("hi", "Olá")])
            .with_fallback("pt-BR");
        assert_eq!(resources.get(&Locale::new("de"), "hi"), Some("Olá"));
    }

    #[test]
    fn replacing_a_locale_replaces_all_of_its_strings() {
        let resources = StringResources::new()
            .with_locale("en", [("a", "1"), ("b", "2")])
            .with_locale("en", [("a", "3")]);
        assert_eq!(resources.get(&Locale::new("en"), "a"), Some("3"));
        assert_eq!(resources.get(&Locale::new("en"), "b"), None);
    }

    #[test]
    fn underscores_in_tags_are_hyphens() {
        let locale = Locale::new("pt_BR");
        assert_eq!(locale.tag(), "pt-BR");
        assert_eq!(locale, Locale::new("pt-BR"));
        assert_eq!(locale.region(), Some("BR"));
    }

    #[test]
    fn region_and_script_are_found_wherever_they_are() {
        let locale = Locale::new("zh-Hans-CN");
        assert_eq!(locale.script(), Some("Hans"));
        assert_eq!(locale.region(), Some("CN"));
        assert_eq!(Locale::new("es-419").region(), Some("419"));
        assert_eq!(Locale::new("en").region(), None);
        assert_eq!(Locale::new("en").script(), None);
    }

    #[test]
    fn the_script_decides_the_direction_when_the_tag_names_one() {
        assert_eq!(
            Locale::new("ur-Latn").layout_direction(),
            LayoutDirection::Ltr
        );
        assert_eq!(
            Locale::new("az-Arab").layout_direction(),
            LayoutDirection::Rtl
        );
        assert_eq!(
            Locale::new("he-IL").layout_direction(),
            LayoutDirection::Rtl
        );
        assert_eq!(Locale::new("AR").layout_direction(), LayoutDirection::Rtl);
        assert_eq!(
            Locale::new("ja-JP").layout_direction(),
            LayoutDirection::Ltr
        );
    }

    #[test]
    fn locale_reports_language_region_and_direction() {
        let locale = Locale::new("ar-EG");
        assert_eq!(locale.language(), "ar");
        assert_eq!(locale.region(), Some("EG"));
        assert_eq!(locale.layout_direction(), LayoutDirection::Rtl);
        assert_eq!(Locale::new("en").layout_direction(), LayoutDirection::Ltr);
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    /// Runs `content` inside a composition, where composition locals can be read.
    fn composed(content: impl FnMut() + 'static) {
        rustle_ui::composition(content).compose();
    }

    fn resources() -> StringResources {
        StringResources::new()
            .with_locale(
                "en",
                [
                    ("hello", "Hello, {0}!"),
                    ("count", "{0} of {1}"),
                    ("plain", "Plain"),
                ],
            )
            .with_locale("fr", [("hello", "Bonjour, {0} !")])
    }

    #[test]
    fn a_resource_follows_the_locale_in_effect() {
        composed(|| {
            provide_string_resources(resources(), || {
                provide_locale(Locale::new("fr-CA"), || {
                    assert_eq!(string_resource_with("hello", &[&"Zoé"]), "Bonjour, Zoé !");
                    assert_eq!(string_resource("plain"), "Plain");
                });
                provide_locale(Locale::new("en"), || {
                    assert_eq!(string_resource_with("hello", &[&"Zoé"]), "Hello, Zoé!");
                });
            });
        });
    }

    #[test]
    fn a_missing_key_shows_as_itself() {
        composed(|| {
            provide_string_resources(resources(), || {
                provide_locale(Locale::new("en"), || {
                    assert_eq!(string_resource("no.such.key"), "no.such.key");
                    assert_eq!(string_resource_with("no {0}", &[&1]), "no 1");
                });
            });
        });
    }

    #[test]
    fn arguments_are_put_in_their_positions() {
        composed(|| {
            provide_string_resources(resources(), || {
                provide_locale(Locale::new("en"), || {
                    assert_eq!(string_resource_with("count", &[&3, &"ten"]), "3 of ten");
                });
            });
        });
    }

    #[test]
    fn an_argument_that_looks_like_a_placeholder_is_left_alone() {
        composed(|| {
            provide_string_resources(resources(), || {
                provide_locale(Locale::new("en"), || {
                    assert_eq!(string_resource_with("count", &[&"{1}", &"x"]), "{1} of x");
                });
            });
        });
    }

    #[test]
    fn a_placeholder_without_an_argument_stays_and_stray_braces_are_kept() {
        assert_eq!(fill_placeholders("a {1} b", &[&0]), "a {1} b");
        assert_eq!(fill_placeholders("{ {0} }", &[&"x"]), "{ x }");
        assert_eq!(fill_placeholders("{0", &[&"x"]), "{0");
        assert_eq!(fill_placeholders("{{0}}", &[&"x"]), "{x}");
        assert_eq!(fill_placeholders("{x} {0}", &[&"y"]), "{x} y");
    }

    #[test]
    fn providing_a_locale_mirrors_right_to_left_layouts() {
        composed(|| {
            provide_locale(Locale::new("he"), || {
                assert_eq!(LOCAL_LAYOUT_DIRECTION.current(), LayoutDirection::Rtl);
                provide_locale(Locale::new("en"), || {
                    assert_eq!(LOCAL_LAYOUT_DIRECTION.current(), LayoutDirection::Ltr);
                });
            });
            assert_eq!(LOCAL_LOCALE.current(), Locale::system());
        });
    }

    #[test]
    fn resources_compare_by_identity_and_fallback() {
        let first = resources();
        assert_eq!(first, first.clone());
        assert_ne!(first, resources());
        assert_ne!(first, first.clone().with_fallback("fr"));
    }
}
