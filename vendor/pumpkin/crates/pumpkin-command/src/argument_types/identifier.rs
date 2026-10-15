use pumpkin_util::identifier::Identifier;

use crate::{
    argument_types::{
        FromStringReader,
        argument_type::{ArgumentType, JavaClientArgumentType},
    },
    errors::command_syntax_error::CommandSyntaxError,
    string_reader::StringReader,
};

/// An argument type that parses a generic [`Identifier`] with a namespace and path.
pub struct IdentifierArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for IdentifierArgumentType {
    type Item = Identifier;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        Identifier::from_reader(reader)
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::ResourceLocation
    }

    fn examples(&self) -> Vec<String> {
        examples!("foo", "foo:bar")
    }
}

impl IdentifierArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &crate::context::command_context::CommandContext<S>,
        name: &str,
    ) -> Result<Identifier, CommandSyntaxError> {
        Ok(context.get_argument::<Identifier>(name)?.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::DummySource;

    #[test]
    fn namespace_parent_component_is_rejected_without_rejecting_dot_or_dotted_names() {
        for input in ["..:probe", "..:folder/probe"] {
            let mut reader = StringReader::new(input);
            assert!(
                <IdentifierArgumentType as ArgumentType<DummySource>>::parse(
                    &IdentifierArgumentType,
                    &mut reader
                )
                .is_err()
            );
            assert_eq!(reader.cursor(), 0);
        }
        for input in [
            ".:probe",
            "dots..inside:probe",
            "minecraft:..",
            "valid:folder/probe",
        ] {
            let mut reader = StringReader::new(input);
            assert!(
                <IdentifierArgumentType as ArgumentType<DummySource>>::parse(
                    &IdentifierArgumentType,
                    &mut reader
                )
                .is_ok(),
                "{input}"
            );
            assert_eq!(reader.remaining_length(), 0);
        }
    }
}
