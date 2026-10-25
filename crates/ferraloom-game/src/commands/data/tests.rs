use super::*;
use crate::commands::{
    CommandSourceStack, Commands, exceptions::error_component, storage::CommandStorage,
};
use ferraloom_util::Vec3;
use serde::Deserialize;

struct StorageHost(CommandStorage);
impl DataAccess for StorageHost {
    fn resolve(
        &mut self,
        command: &PreparedCommand,
        kind: &str,
        argument: &str,
    ) -> Result<DataAccessor, DataError> {
        assert_eq!(kind, "storage", "storage-only native fixture");
        Ok(DataAccessor::Storage(
            Identifier::parse(command.string(argument).unwrap()).unwrap(),
        ))
    }
    fn get_data(&mut self, accessor: &DataAccessor) -> Result<CompoundTag, DataError> {
        let DataAccessor::Storage(id) = accessor else {
            unreachable!()
        };
        Ok(self.0.get(id)?)
    }
    fn set_data(&mut self, accessor: &DataAccessor, value: CompoundTag) -> Result<(), DataError> {
        let DataAccessor::Storage(id) = accessor else {
            unreachable!()
        };
        Ok(self.0.set(id, value)?)
    }
    fn publish_read_mutation(
        &mut self,
        accessor: &DataAccessor,
        value: &CompoundTag,
    ) -> Result<(), DataError> {
        let DataAccessor::Storage(id) = accessor else {
            unreachable!()
        };
        Ok(self.0.publish_read_mutation(id, value)?)
    }
    fn display_name(&self, _: EntityRef) -> Result<Component, DataError> {
        unreachable!("storage-only native fixture")
    }
}
#[derive(Deserialize)]
struct Vector {
    command: String,
    initial: String,
    source: String,
    value: Option<i32>,
    error: Option<serde_json::Value>,
    feedback: serde_json::Value,
    root: String,
    absent: String,
    dirty: bool,
    keys: Vec<String>,
}
#[test]
fn native_storage_data_commands_preserve_results_feedback_partial_changes_and_dirty_flags() {
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../vanilla/data-command-vectors.json")).unwrap();
    let commands = Commands::default();
    for vector in &vectors {
        let mut host = StorageHost(CommandStorage::default());
        let root = Identifier::parse("test:root").unwrap();
        host.0
            .set(
                &root,
                ferraloom_nbt::parse_compound(&vector.initial).unwrap(),
            )
            .unwrap();
        host.0
            .set(
                &Identifier::parse("test:source").unwrap(),
                ferraloom_nbt::parse_compound(&vector.source).unwrap(),
            )
            .unwrap();
        host.0.take_dirty();
        let result = commands
            .prepare(&vector.command, CommandSourceStack::server(4, Vec3::ZERO))
            .map_err(DataError::from)
            .and_then(|command| execute(&command, &mut host));
        match result {
            Ok(result) => {
                assert_eq!(Some(result.value), vector.value, "{}", vector.command);
                let feedback=result.feedback.iter().map(|feedback|serde_json::json!({"message":serde_json::from_str::<serde_json::Value>(&feedback.message.to_json().unwrap()).unwrap(),"broadcast":feedback.broadcast})).collect::<Vec<_>>();
                assert_eq!(
                    serde_json::json!(feedback),
                    vector.feedback,
                    "{}",
                    vector.command
                );
                assert!(vector.error.is_none());
            }
            Err(DataError::Syntax(error)) => {
                let actual: serde_json::Value =
                    serde_json::from_str(&error_component(&error).to_json().unwrap()).unwrap();
                assert_eq!(Some(actual), vector.error, "{}", vector.command);
            }
            Err(error) => panic!("{}: {error:?}", vector.command),
        }
        assert_eq!(
            host.0.get(&root).unwrap(),
            ferraloom_nbt::parse_compound(&vector.root).unwrap(),
            "{}",
            vector.command
        );
        assert_eq!(
            host.0
                .get(&Identifier::parse("test:absent").unwrap())
                .unwrap(),
            ferraloom_nbt::parse_compound(&vector.absent).unwrap(),
            "{}",
            vector.command
        );
        assert_eq!(
            !host.0.take_dirty().is_empty(),
            vector.dirty,
            "{}",
            vector.command
        );
        assert_eq!(
            host.0
                .keys()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vector.keys,
            "{}",
            vector.command
        );
    }
    assert_eq!(vectors.len(), 166);
}
