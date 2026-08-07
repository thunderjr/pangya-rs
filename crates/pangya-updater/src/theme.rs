//! The client's "extra contents" documents: notice images and lobby/loading wallpapers.
//!
//! The U.S. 852 client fetches `extracontents.xml`, follows its `url` attribute to a theme
//! document, then downloads every image the theme names. The `url` is passed to WinINet
//! verbatim, so it has to be an absolute URL built from the operator's advertised address —
//! a leading-slash path makes the client issue a request with no host and fail.

/// Errors from building a theme document.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    /// An image name was not usable in a URL the client will request.
    ///
    /// The client appends the name to the theme's base URL with no escaping, so a name with a
    /// path separator or a URL delimiter in it would send the client somewhere else entirely.
    #[error("a theme image name is not usable in a client URL")]
    ImageName,
    /// The advertised base URL did not end in a separator.
    #[error("the advertised theme base URL must end in '/'")]
    BaseUrl,
}

/// A theme: the images the client will be told to fetch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    /// Notice images, newest first.
    pub notices: Vec<String>,
    /// Lobby wallpapers.
    pub lobby_wallpapers: Vec<String>,
    /// Loading-screen wallpapers.
    pub loading_wallpapers: Vec<String>,
}

/// Rejects a name the client could not safely append to a base URL.
fn validate_image_name(name: &str) -> Result<(), ThemeError> {
    let acceptable = !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        && name != "."
        && name != ".."
        && !name.contains("..");
    if acceptable {
        Ok(())
    } else {
        Err(ThemeError::ImageName)
    }
}

impl Theme {
    /// Validates every image name.
    ///
    /// # Errors
    /// Returns [`ThemeError::ImageName`] for a name that is not a plain file name.
    pub fn validate(&self) -> Result<(), ThemeError> {
        for name in self
            .notices
            .iter()
            .chain(&self.lobby_wallpapers)
            .chain(&self.loading_wallpapers)
        {
            validate_image_name(name)?;
        }
        Ok(())
    }

    /// Renders the theme document.
    ///
    /// # Errors
    /// Returns [`ThemeError::ImageName`] for a name that is not a plain file name.
    pub fn to_xml(&self) -> Result<String, ThemeError> {
        self.validate()?;
        let mut out = String::from("<?xml version=\"1.0\" standalone=\"yes\"?>\n<extracontents>\n");
        out.push_str("  <topicons />\n");
        out.push_str("  <notice>\n");
        for name in &self.notices {
            out.push_str(&format!("    <file name=\"{name}\" />\n"));
        }
        out.push_str("  </notice>\n");
        for (tag, wallpapers) in [
            ("lobby", &self.lobby_wallpapers),
            ("loading", &self.loading_wallpapers),
        ] {
            out.push_str(&format!("  <{tag}>\n"));
            for name in wallpapers {
                out.push_str(&format!(
                    "    <wallpaper prob=\"1\"><file name=\"{name}\"/></wallpaper>\n"
                ));
            }
            out.push_str(&format!("  </{tag}>\n"));
        }
        out.push_str("</extracontents>\n");
        Ok(out)
    }
}

/// Renders the `extracontents.xml` index that points the client at the theme document.
///
/// `base_url` must be absolute and end in `/`; the client concatenates the `src` onto it.
///
/// # Errors
/// Returns [`ThemeError::BaseUrl`] when the base URL is not a usable prefix.
pub fn extra_contents_xml(base_url: &str) -> Result<String, ThemeError> {
    let usable = base_url.ends_with('/')
        && (base_url.starts_with("http://") || base_url.starts_with("https://"))
        && !base_url.contains('"')
        && !base_url.contains('<')
        && !base_url.contains('>');
    if !usable {
        return Err(ThemeError::BaseUrl);
    }
    Ok(format!(
        "<?xml version=\"1.0\" standalone=\"yes\"?>\n\
         <extracontents>\n\
         \x20 <themes>\n\
         \x20   <pangya_default src=\"pangya_default.xml\" url=\"{base_url}\" />\n\
         \x20 </themes>\n\
         </extracontents>\n"
    ))
}

/// Encodes a translation catalog the way the client's `Translation/Read.aspx` returns it.
///
/// The client base64-decodes the whole body and parses a flat `<TEXT><KEY/><DEV/><SERVICE/>`
/// sequence. The catalog text itself is operator-supplied: it is the client's own localized
/// strings, so this crate transports it rather than shipping it.
#[must_use]
pub fn encode_translation_catalog(catalog_xml: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(catalog_xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_document_lists_every_section() {
        let theme = Theme {
            notices: vec!["2016041502.jpg".to_owned()],
            lobby_wallpapers: vec!["main_bg.jpg".to_owned(), "main_bg_2.jpg".to_owned()],
            loading_wallpapers: vec!["background_01.jpg".to_owned()],
        };
        let xml = theme.to_xml().expect("valid");
        assert!(xml.contains("<file name=\"2016041502.jpg\" />"));
        assert!(xml.contains("<wallpaper prob=\"1\"><file name=\"main_bg.jpg\"/></wallpaper>"));
        assert!(
            xml.contains("<wallpaper prob=\"1\"><file name=\"background_01.jpg\"/></wallpaper>")
        );
        assert_eq!(xml.matches("<wallpaper").count(), 3);
    }

    #[test]
    fn an_empty_theme_is_still_a_well_formed_document() {
        let xml = Theme::default().to_xml().expect("valid");
        assert!(xml.starts_with("<?xml version=\"1.0\" standalone=\"yes\"?>"));
        assert!(xml.contains("<lobby>\n  </lobby>"));
    }

    #[test]
    fn traversal_and_delimiter_names_are_rejected() {
        for name in [
            "../secret.jpg",
            "a/b.jpg",
            "a\\b.jpg",
            "a?b.jpg",
            "a b.jpg",
            "..",
            "",
            "a..b",
        ] {
            let theme = Theme {
                notices: vec![name.to_owned()],
                ..Theme::default()
            };
            assert!(
                matches!(theme.to_xml(), Err(ThemeError::ImageName)),
                "{name:?} was accepted"
            );
        }
    }

    #[test]
    fn plain_names_are_accepted() {
        let theme = Theme {
            notices: vec!["main_bg-2_v1.jpg".to_owned()],
            ..Theme::default()
        };
        assert!(theme.to_xml().is_ok());
    }

    #[test]
    fn extra_contents_requires_an_absolute_url_with_a_trailing_separator() {
        assert!(extra_contents_xml("http://127.0.0.1:8080/theme/").is_ok());
        assert!(extra_contents_xml("https://example.test/a/b/").is_ok());
        assert!(matches!(
            extra_contents_xml("/theme/"),
            Err(ThemeError::BaseUrl)
        ));
        assert!(matches!(
            extra_contents_xml("http://127.0.0.1:8080/theme"),
            Err(ThemeError::BaseUrl)
        ));
        assert!(matches!(
            extra_contents_xml("http://a/\"/"),
            Err(ThemeError::BaseUrl)
        ));
    }

    #[test]
    fn extra_contents_embeds_the_base_url_verbatim() {
        let xml = extra_contents_xml("http://100.64.0.1:8080/S4_Patch/extracontents/default/")
            .expect("valid");
        assert!(xml.contains(
            "<pangya_default src=\"pangya_default.xml\" \
             url=\"http://100.64.0.1:8080/S4_Patch/extracontents/default/\" />"
        ));
    }

    #[test]
    fn translation_catalog_is_standard_base64() {
        assert_eq!(
            encode_translation_catalog(b"<TEXT></TEXT>"),
            "PFRFWFQ+PC9URVhUPg=="
        );
        assert_eq!(encode_translation_catalog(b""), "");
    }
}
