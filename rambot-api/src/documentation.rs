use std::fmt::{self, Display, Formatter};

/// Documentation for any audio (audio source or audio source list) to be
/// displayed to the user of the bot. The overview entry can be accessed by
/// [AudioDocumentation::overview_entry] while a long description page is
/// available behind the implementation of the [Display] trait. Both versions
/// use markdown.
///
/// To construct instances of this type, use the [AudioDocumentationBuilder].
pub struct AudioDocumentation {
    name: String,
    summary: String,
    description: String
}

impl AudioDocumentation {

    /// Gets the name of the documented audio.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets an entry for an overview list of many audios. This contains the
    /// name of the audio as well as a short summary. Uses markdown for
    /// formatting.
    pub fn overview_entry(&self) -> String {
        format!("**{}**: {}", &self.name, &self.summary)
    }
}

impl Display for AudioDocumentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "**{}**\n\n{}", &self.name, &self.description)
    }
}

/// A builder for [AudioDocumentation]s. To construct an audio documentation,
/// create a new builder using [AudioDocumentationBuilder::new], specify at
/// least a name and summary using [AudioDocumentationBuilder::with_name] and
/// [AudioDocumentationBuilder::with_summary] or
/// [AudioDocumentationBuilder::set_name] and
/// [AudioDocumentationBuilder::set_summary] respectively, and then build the
/// final documentation using [AudioDocumentationBuilder::build]. Further
/// information can be provided with other methods.
///
/// A simple usage example is shown below.
///
/// ```
/// use rambot_api::AudioDocumentationBuilder;
///
/// // Documentation for a folder playlist
///
/// let doc = AudioDocumentationBuilder::new()
///     .with_name("folder-list")
///     .with_summary("Plays all audio files in a given folder.")
///     .build();
/// ```
pub struct AudioDocumentationBuilder {
    name: Option<String>,
    summary: Option<String>,
    description: Option<String>
}

impl AudioDocumentationBuilder {

    /// Creates a new audio documentation builder.
    pub fn new() -> AudioDocumentationBuilder {
        AudioDocumentationBuilder {
            name: None,
            summary: None,
            description: None
        }
    }

    /// Specify a name for this audio. Calling this function or
    /// [AudioDocumentationBuilder::with_name] before building is mandatory.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of this audio. Markdown is supported.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn set_name<S>(&mut self, name: S) -> &mut AudioDocumentationBuilder
    where
        S: Into<String>
    {
        self.name = Some(name.into());
        self
    }

    /// Specify a name for this audio. Calling this function or
    /// [AudioDocumentationBuilder::set_name] before building is mandatory.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of this audio. Markdown is supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_name<S>(mut self, name: S) -> AudioDocumentationBuilder
    where
        S: Into<String>
    {
        self.set_name(name);
        self
    }

    /// Specify a short summary for this audio to be displayed in the overview
    /// page. If no long description has been assigned yet, it will be set to
    /// that same summary. Calling this function or
    /// [AudioDocumentationBuilder::with_summary] before building is mandatory.
    ///
    /// # Arguments
    ///
    /// * `summary`: A short summary for this audio. Markdown is supported.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn set_summary<S>(&mut self, summary: S) ->
        &mut AudioDocumentationBuilder
    where
        S: Into<String>
    {
        let summary = summary.into();

        self.description.get_or_insert_with(|| summary.clone());
        self.summary = Some(summary);
        self
    }

    /// Specify a short summary for this audio to be displayed in the overview
    /// page. If no long description has been assigned yet, it will be set to
    /// that same summary. Calling this function or
    /// [AudioDocumentationBuilder::set_summary] before building is mandatory.
    ///
    /// # Arguments
    ///
    /// * `summary`: A short summary for this audio. Markdown is supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_summary<S>(mut self, summary: S) -> AudioDocumentationBuilder
    where
        S: Into<String>
    {
        self.set_summary(summary);
        self
    }

    /// Specify a longer description for this audio to be displayed in a
    /// dedicated documentation page.
    ///
    /// # Arguments
    ///
    /// * `description`: A longer description for this audio. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn set_description<S>(&mut self, description: S) ->
        &mut AudioDocumentationBuilder
    where
        S: Into<String>
    {
        self.description = Some(description.into());
        self
    }

    /// Specify a longer description for this audio to be displayed in a
    /// dedicated documentation page.
    ///
    /// # Arguments
    ///
    /// * `description`: A longer description for this audio. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_description<S>(mut self, description: S)
        -> AudioDocumentationBuilder
    where
        S: Into<String>
    {
        self.set_description(description);
        self
    }

    /// Builds the audio documentation constructed from the data provided with
    /// previous method calls. At least [AudioDocumentationBuilder::with_name]
    /// and [AudioDocumentationBuilder::with_summary] are required to be called
    /// before this.
    ///
    /// # Returns
    ///
    /// `Some(_)` with a new [AudioDocumentation] instance with the previously
    /// provided information. If no name or no summary has been specified,
    /// `None` is returned.
    pub fn build(self) -> Option<AudioDocumentation> {
        let parts = (self.name, self.summary, self.description);

        if let (Some(name), Some(summary), Some(description)) = parts {
            Some(AudioDocumentation {
                name,
                summary,
                description
            })
        }
        else {
            None
        }
    }
}

impl Default for AudioDocumentationBuilder {
    fn default() -> AudioDocumentationBuilder {
        AudioDocumentationBuilder::new()
    }
}

struct ModifierParameterDocumentation {
    name: String,
    description: String
}

impl Display for ModifierParameterDocumentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", &self.name, &self.description)
    }
}

/// Documentation of a modifier (effect or adapter) to be displayed to the user
/// of the bot. The short form can be accessed by
/// [ModifierDocumentation::short_summary] while a long markdown version is
/// available behind the implementation of the [Display] trait.
///
/// To construct instances of this type, use the
/// [ModifierDocumentationBuilder].
pub struct ModifierDocumentation {
    short_summary: String,
    long_summary: String,
    parameters: Vec<ModifierParameterDocumentation>
}

impl ModifierDocumentation {

    /// Gets a short summary of the functionality of the documented modifier.
    /// This is used for the overview page.
    pub fn short_summary(&self) -> &str {
        &self.short_summary
    }
}

impl Display for ModifierDocumentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.long_summary)?;

        if self.parameters.is_empty() {
            return Ok(());
        }

        writeln!(f)?;

        for parameter in &self.parameters {
            write!(f, "\n- {}", parameter)?;
        }

        Ok(())
    }
}

/// A builder for [ModifierDocumentation]s. To construct a modifier
/// documentation, create a new builder using
/// [ModifierDocumentationBuilder::new], specify at least a short summary
/// using [ModifierDocumentationBuilder::with_short_summary], and then build
/// the final documentation using [ModifierDocumentationBuilder::build].
/// Further information can be provided with other methods. You do not need to
/// provide the effect/adapter name, as that is taken from context.
///
/// A simple usage example is shown below.
///
/// ```
/// use rambot_api::ModifierDocumentationBuilder;
///
/// // Documentation for a volume effect
///
/// let doc = ModifierDocumentationBuilder::new()
///     .with_short_summary("Controls the volume of a layer.")
///     .with_long_summary(
///         "Controls the volume of a layer by multiplying all audio with a \
///         given factor.")
///     .with_parameter("volume", "The factor by which audio is multiplied.")
///     .build();
/// ```
pub struct ModifierDocumentationBuilder {
    short_summary: Option<String>,
    long_summary: Option<String>,
    parameters: Vec<ModifierParameterDocumentation>
}

impl ModifierDocumentationBuilder {

    /// Creates a new modifier documentation builder.
    pub fn new() -> ModifierDocumentationBuilder {
        ModifierDocumentationBuilder {
            short_summary: None,
            long_summary: None,
            parameters: Vec::new()
        }
    }

    /// Specify a short summary for this effect/adapter to be displayed in the
    /// overview. If no long summary has been specified, it will be assigned to
    /// the given short summary as well.
    ///
    /// # Arguments
    ///
    /// * `summary`: A short summary for this effect/adapter. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn set_short_summary<S>(&mut self, summary: S)
        -> &mut ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        let summary = summary.into();

        self.long_summary.get_or_insert_with(|| summary.clone());
        self.short_summary = Some(summary);
        self
    }

    /// Specify a short summary for this effect/adapter to be displayed in the
    /// overview. If no long summary has been specified, it will be assigned to
    /// the given short summary as well.
    ///
    /// # Arguments
    ///
    /// * `summary`: A short summary for this effect/adapter. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_short_summary<S>(mut self, summary: S)
        -> ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        self.set_short_summary(summary);
        self
    }

    /// Specify a long summary for this effect/adapter to be displayed in the
    /// effect/adapter specific help page.
    ///
    /// # Arguments
    ///
    /// * `summary`: A long summary for this effect/adapter. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn set_long_summary<S>(&mut self, summary: S)
        -> &mut ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        self.long_summary = Some(summary.into());
        self
    }

    /// Specify a long summary for this effect/adapter to be displayed in the
    /// effect/adapter specific help page.
    ///
    /// # Arguments
    ///
    /// * `summary`: A long summary for this effect/adapter. Markdown is
    ///   supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_long_summary<S>(mut self, summary: S)
        -> ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        self.set_long_summary(summary);
        self
    }

    /// Adds a parameter documentation for a new parameter to the constructed
    /// modifier documentation. To add multiple parameters, call this method or
    /// [ModifierDocumentationBuilder::with_parameter] multiple times. The
    /// parameters will be displayed top-to-bottom in the order these method
    /// are called.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the documented parameter.
    /// * `description`: A description to be displayed for the documented
    ///   parameter.
    ///
    /// # Returns
    ///
    /// A mutable reference to this builder after the operation. Useful for
    /// chaining.
    pub fn add_parameter<S1, S2>(&mut self, name: S1, description: S2)
        -> &mut ModifierDocumentationBuilder
    where
        S1: Into<String>,
        S2: Into<String>
    {
        let name = name.into();
        let description = description.into();

        self.parameters.push(ModifierParameterDocumentation {
            name,
            description
        });

        self
    }

    /// Adds a parameter documentation for a new parameter to the constructed
    /// modifier documentation. To add multiple parameters, call this method or
    /// [ModifierDocumentationBuilder::add_parameter] multiple times. The
    /// parameters will be displayed top-to-bottom in the order these method
    /// are called.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the documented parameter.
    /// * `description`: A description to be displayed for the documented
    ///   parameter.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_parameter<S1, S2>(mut self, name: S1, description: S2)
        -> ModifierDocumentationBuilder
    where
        S1: Into<String>,
        S2: Into<String>
    {
        let name = name.into();
        let description = description.into();

        self.parameters.push(ModifierParameterDocumentation {
            name,
            description
        });

        self
    }

    /// Builds the modifier documentation constructed from the data provided
    /// with previous method calls. At least
    /// [ModifierDocumentationBuilder::with_short_summary] is required to be
    /// called before this.
    ///
    /// # Returns
    ///
    /// `Some(_)` with a new [ModifierDocumentation] instance with the
    /// previously provided information. If no short summary has been
    /// specified, `None` is returned.
    pub fn build(self) -> Option<ModifierDocumentation> {
        self.short_summary
            .and_then(|short| self.long_summary.map(|long| (short, long)))
            .map(|(short_summary, long_summary)| ModifierDocumentation {
                short_summary,
                long_summary,
                parameters: self.parameters
            })
    }
}

impl Default for ModifierDocumentationBuilder {
    fn default() -> ModifierDocumentationBuilder {
        ModifierDocumentationBuilder::new()
    }
}
