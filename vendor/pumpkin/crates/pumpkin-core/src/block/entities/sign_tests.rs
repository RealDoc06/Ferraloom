//! Real dispatcher and production sign-writer regressions. Deadlock limits bound
//! failures; channels and try_lock establish ordering without timing assumptions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::block::blocks::signs::execute_sign_click_commands;
use crate::block::entities::hanging_sign::HangingSignBlockEntity;
use crate::command::commands::data::tests::Fixture;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_data::Block;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POS: BlockPos = BlockPos::new(0, 64, 0);
const DEADLOCK_GUARD: Duration = Duration::from_secs(10);

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(nbt) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected compound fixture");
    };
    nbt
}

fn saved(entity: &dyn BlockEntity) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    entity.write_internal(&mut nbt);
    nbt
}

fn entity(hanging: bool, nbt: &NbtCompound) -> (&'static Block, Arc<dyn BlockEntity>) {
    if hanging {
        (
            &Block::OAK_HANGING_SIGN,
            Arc::new(HangingSignBlockEntity::from_nbt(nbt, POS)),
        )
    } else {
        (
            &Block::OAK_SIGN,
            Arc::new(SignBlockEntity::from_nbt(nbt, POS)),
        )
    }
}

#[tokio::test]
async fn sign_structured_lines_preserve_styles_positions_flags_and_real_chunk_reload() {
    let fixture = Fixture::new().await;
    for hanging in [false, true] {
        let (block, live) = entity(hanging, &NbtCompound::new());
        fixture.install(block, live.clone());
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        let front = sign.front_text().clone();
        fixture.execute("data merge block 0 64 0 {front_text:{messages:[{text:'A',color:'red',bold:1b,italic:0b,extra:[{text:'!',underlined:1b}]},'B',{text:'C'},'D']},allow_op_features:1b}").unwrap();
        assert_eq!(
            sign.front_text().get_messages(false).map(String::from),
            ["A!", "B", "C", "D"]
        );
        assert!(front.shares_message_storage(sign.front_text(), false));
        let snapshot = fixture.block_data();
        let lines = snapshot
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        assert_eq!(lines[1].extract_string(), Some("B"));
        assert_eq!(lines[2].extract_string(), Some("C"));
        assert_eq!(lines[3].extract_string(), Some("D"));
        let first = lines[0].extract_compound().unwrap();
        assert_eq!(first.get_string("color"), Some("red"));
        assert_eq!(first.get_bool("bold"), Some(true));
        assert_eq!(first.get_bool("italic"), Some(false));
        assert_eq!(snapshot.get_bool("allow_op_features"), Some(true));
        assert_eq!(fixture.pending_data(), snapshot);
        let reloaded = fixture.reload_saved_chunk();
        assert_eq!(saved(reloaded.as_ref()), snapshot);
        assert_eq!(reloaded.chunk_data_nbt(), live.chunk_data_nbt());

        // A bad raw list resets the complete side. A bad filtered list is a
        // lenient optional codec field and falls back to the four raw lines.
        fixture.execute("data merge block 0 64 0 {front_text:{messages:['bad','count'],color:red,has_glowing_text:1b},back_text:{messages:['one','two','three','four'],filtered_messages:['bad']}}").unwrap();
        let sign = SignEntityRef::from_block_entity(reloaded.as_ref()).unwrap();
        assert_eq!(
            sign.front_text().get_messages(false),
            SignText::empty_messages()
        );
        assert_eq!(sign.front_text().get_color(), DyeColor::Black);
        assert!(!sign.front_text().has_glowing_text());
        assert_eq!(
            sign.back_text().get_messages(true),
            sign.back_text().get_messages(false)
        );
    }
}

#[tokio::test]
async fn sign_unknown_components_are_preserved_but_live_edits_refuse_without_publication() {
    let fixture = Fixture::new().await;
    for hanging in [false, true] {
        let unknown = compound(
            "{front_text:{messages:['first',{translate:'chat.type.text',with:['one','two']},'third','fourth']},allow_op_features:1b}",
        );
        let (block, live) = entity(hanging, &unknown);
        fixture.install(block, live.clone());
        let before = fixture.block_data();
        let pending = fixture.pending_data();
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        assert_eq!(sign.front_text().get_message(0, false).as_ref(), "first");
        assert_eq!(sign.front_text().get_message(2, false).as_ref(), "third");
        let lines = before
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        assert_eq!(
            &lines[1],
            &unknown
                .get_compound("front_text")
                .unwrap()
                .get_list("messages")
                .unwrap()[1]
        );
        assert!(
            fixture
                .execute("data merge block 0 64 0 {is_waxed:1b}")
                .is_err()
        );
        assert_eq!(fixture.block_data(), before);
        assert_eq!(fixture.pending_data(), pending);
        assert!(!sign.is_waxed());
        let client = client_nbt_from_persisted(&before);
        for key in ["id", "x", "y", "z", "is_waxed", "allow_op_features"] {
            assert_eq!(client.get(key), before.get(key));
        }
        let client_lines = client
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        assert_eq!(
            SignLine::from_nbt(&client_lines[1]).text().as_ref(),
            "[Unsupported sign text]"
        );
        assert_eq!(
            SignLine::from_nbt(&client_lines[2]).text().as_ref(),
            "third"
        );
        let reloaded = fixture.reload_saved_chunk();
        assert_eq!(saved(reloaded.as_ref()), before);
    }
}

#[derive(Clone, Copy)]
enum Writer {
    Text,
    Dye,
    Glow,
    Wax,
}

fn write(sign: &SignEntityRef<'_>, writer: Writer, editor: uuid::Uuid) -> bool {
    match writer {
        Writer::Text => sign.update_player_text(
            editor,
            true,
            false,
            ["new", "two", "three", "four"].map(Box::from),
        ),
        Writer::Dye => sign.apply_color(editor, true, DyeColor::Red),
        Writer::Glow => sign.apply_glowing(editor, true, true),
        Writer::Wax => sign.apply_wax(editor),
    }
}

fn assert_writer(sign: &SignEntityRef<'_>, writer: Writer) {
    match writer {
        Writer::Text => {
            assert_eq!(sign.front_text().get_message(0, false).as_ref(), "new");
            assert_eq!(
                sign.front_text().get_messages(false),
                sign.front_text().get_messages(true)
            );
        }
        Writer::Dye => assert_eq!(sign.front_text().get_color(), DyeColor::Red),
        Writer::Glow => assert!(sign.front_text().has_glowing_text()),
        Writer::Wax => assert!(sign.is_waxed()),
    }
    assert_eq!(sign.back_text().get_color(), DyeColor::Blue);
}

#[tokio::test]
async fn sign_dispatcher_edit_serializes_actual_packet_item_writers_and_save_publication() {
    for hanging in [false, true] {
        for writer in [Writer::Text, Writer::Dye, Writer::Glow, Writer::Wax] {
            let fixture = Arc::new(Fixture::new().await);
            let (block, live) = entity(
                hanging,
                &compound("{front_text:{messages:['old','','','']}}"),
            );
            fixture.install(block, live.clone());
            let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
            let editor = uuid::Uuid::from_u128(71);
            *sign.currently_editing_player().lock().unwrap() = Some(editor);
            let retained = sign.front_text().clone();
            let (paused_tx, paused_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let resume_rx = Mutex::new(resume_rx);
            sign.on_next_edit(move || {
                paused_tx.send(()).unwrap();
                resume_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(DEADLOCK_GUARD)
                    .unwrap();
            });
            let command_fixture = fixture.clone();
            let (command_tx, command_rx) = mpsc::channel();
            let command = std::thread::spawn(move || {
                command_tx
                    .send(
                        command_fixture
                            .execute("data modify block 0 64 0 back_text.color set value blue")
                            .map_err(|error| format!("{error:?}")),
                    )
                    .unwrap();
            });
            paused_rx.recv_timeout(DEADLOCK_GUARD).unwrap();
            let writer_entity = live.clone();
            let world = fixture.source.world().clone();
            let (entry_tx, entry_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let gameplay = std::thread::spawn(move || {
                let sign = SignEntityRef::from_block_entity(writer_entity.as_ref()).unwrap();
                entry_tx.send(sign.mutation_is_locked()).unwrap();
                assert!(write(&sign, writer, editor));
                world.update_block_entity(&writer_entity);
                world.save_block_entities(POS.chunk_position());
                done_tx.send(()).unwrap();
            });
            assert!(entry_rx.recv_timeout(DEADLOCK_GUARD).unwrap());
            assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            resume_tx.send(()).unwrap();
            assert_eq!(command_rx.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
            done_rx.recv_timeout(DEADLOCK_GUARD).unwrap();
            command.join().unwrap();
            gameplay.join().unwrap();
            assert!(retained.shares_message_storage(sign.front_text(), false));
            assert!(Arc::ptr_eq(
                &fixture.source.world().get_block_entity(&POS).unwrap(),
                &live
            ));
            assert_writer(&sign, writer);
            assert_eq!(fixture.pending_data(), fixture.block_data());
            let reloaded = fixture.reload_saved_chunk();
            assert_writer(
                &SignEntityRef::from_block_entity(reloaded.as_ref()).unwrap(),
                writer,
            );
        }
    }
}

#[tokio::test]
async fn sign_completed_gameplay_batch_survives_later_command_and_editor_rechecks() {
    let fixture = Fixture::new().await;
    for hanging in [false, true] {
        let (block, live) = entity(
            hanging,
            &compound("{front_text:{messages:['old','','','']}}"),
        );
        fixture.install(block, live.clone());
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        let editor = uuid::Uuid::from_u128(72);
        *sign.currently_editing_player().lock().unwrap() = Some(editor);
        assert!(!sign.update_player_text(
            uuid::Uuid::from_u128(73),
            true,
            false,
            SignText::empty_messages()
        ));
        assert!(sign.update_player_text(
            editor,
            true,
            false,
            ["new", "two", "three", "four"].map(Box::from)
        ));
        fixture
            .execute("data modify block 0 64 0 back_text.color set value blue")
            .unwrap();
        assert_writer(&sign, Writer::Text);
        let before = fixture.block_data();
        assert!(!sign.update_player_text(editor, true, false, SignText::empty_messages()));
        assert_eq!(fixture.block_data(), before);
        *sign.currently_editing_player().lock().unwrap() = Some(editor);
        assert!(sign.apply_wax(editor));
        assert!(!sign.update_player_text(editor, true, false, SignText::empty_messages()));
        assert_eq!(sign.front_text().get_message(0, false).as_ref(), "new");
    }
}

/// The parent process guards exactly this test, including the actual self-click
/// dispatcher, heartbeat and save. A deadlock cannot wedge the entire Rust suite.
#[test]
fn sign_click_commands_are_literal_gated_and_reentrant() {
    const CHILD: &str = "FERRALOOM_SIGN_REENTRANCY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "block::entities::sign::tests::sign_click_commands_are_literal_gated_and_reentrant",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "self-click command/save deadlocked: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            // This is only the outer process watchdog, never ordering evidence.
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new().await;
        for hanging in [false, true] {
            let (block, live) = entity(hanging, &NbtCompound::new());
            fixture.install(block, live.clone());
            let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
            let editor = uuid::Uuid::from_u128(74);
            *sign.currently_editing_player().lock().unwrap() = Some(editor);
            let attack = r#"{"text":"","click_event":{"action":"run_command","command":"data merge block 0 64 0 {is_waxed:1b}"}}"#;
            assert!(sign.update_player_text(editor, true, false, [attack, "", "", ""].map(Box::from)));
            // Even setting allow_op_features cannot turn packet text into code.
            fixture.execute("data merge block 0 64 0 {allow_op_features:1b}").unwrap();
            assert!(!execute_sign_click_commands(&sign, true, false, &fixture.source, &fixture.dispatcher));
            assert!(!sign.is_waxed());
            assert_eq!(sign.front_text().get_message(0, false).as_ref(), attack);
            fixture.execute("data merge block 0 64 0 {front_text:{messages:[{text:'unwax',click_event:{action:'run_command',command:'data merge block 0 64 0 {is_waxed:0b}'}},'','','']},is_waxed:1b,allow_op_features:0b}").unwrap();
            assert!(execute_sign_click_commands(&sign, true, false, &fixture.source, &fixture.dispatcher));
            assert!(sign.is_waxed(), "disabled privileged click must not run");
            fixture.execute("data merge block 0 64 0 {allow_op_features:1b}").unwrap();
            assert!(sign.is_waxed(), "self-click must have a real state transition");
            assert!(execute_sign_click_commands(&sign, true, false, &fixture.source, &fixture.dispatcher));
            assert!(!sign.is_waxed(), "authorized self-click must execute the command");
            assert_eq!(fixture.execute("data get block 0 64 0 is_waxed").unwrap(), 0);
            fixture.source.world().save_block_entities(POS.chunk_position());
            assert_eq!(fixture.pending_data(), fixture.block_data());
            assert_eq!(saved(fixture.reload_saved_chunk().as_ref()).get_bool("is_waxed"), Some(false));
        }
    });
}

#[test]
fn sign_opaque_mixed_components_round_trip_through_real_nbt_without_client_leakage() {
    use pumpkin_nbt::deserializer::{NbtReadHelperJava, NbtStreamReader};

    let original = compound(
        "{front_text:{messages:['first',{selector:'@a'},{translate:'unknown',extra:['nested',{text:'last'}]},['a',{selector:'@p'}]]},is_waxed:1b,allow_op_features:1b}",
    );
    for hanging in [false, true] {
        let (_, live) = entity(hanging, &original);
        let before = saved(live.as_ref());
        // pumpkin_nbt wraps heterogeneous list entries in compounds and flattens
        // them when reading, including lists inside opaque component payloads.
        let bytes = pumpkin_nbt::Nbt::from(before.clone()).write_unnamed();
        let mut cursor = std::io::Cursor::new(&bytes[..]);
        let mut reader = NbtReadHelperJava::new(NbtStreamReader(&mut cursor));
        let decoded = pumpkin_nbt::Nbt::read_unnamed(&mut reader).unwrap();
        assert_eq!(decoded, before.clone().into());
        let saved_lines = before
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        let original_lines = original
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        assert_eq!(&saved_lines[1..], &original_lines[1..]);
        let client = client_nbt_from_persisted(&before);
        let lines = client
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap();
        assert!(
            lines
                .iter()
                .all(|tag| matches!(tag, NbtTag::String(_) | NbtTag::Compound(_)))
        );
        assert_eq!(SignData::from_nbt(&client).validate(), Ok(()));
        assert_eq!(client.get_bool("is_waxed"), Some(true));
        assert_eq!(client.get_bool("allow_op_features"), Some(true));
    }
}

#[tokio::test]
async fn sign_real_dispatcher_self_and_opposite_sources_complete_and_failed_edits_leave_no_state() {
    let fixture = Fixture::new().await;
    let other_position = BlockPos::new(1, 64, 0);
    for hanging in [false, true] {
        let initial = compound("{front_text:{messages:['abcdef','','','']}}");
        let (block, live) = entity(hanging, &initial);
        fixture.install(block, live.clone());
        let other: Arc<dyn BlockEntity> = if hanging {
            Arc::new(HangingSignBlockEntity::from_nbt(&initial, other_position))
        } else {
            Arc::new(SignBlockEntity::from_nbt(&initial, other_position))
        };
        fixture.install(block, other);
        let execute = |input: &'static str| {
            let source = fixture.source.clone();
            let dispatcher = fixture.dispatcher.clone();
            let (tx, rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                tx.send(
                    dispatcher
                        .execute_input(input, &source)
                        .map_err(|error| format!("{error:?}")),
                )
                .unwrap();
            });
            (worker, rx)
        };
        let (worker, result) = execute(
            "data modify block 0 64 0 back_text.messages[0] set string block 0 64 0 front_text.messages[0] 1 4",
        );
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        worker.join().unwrap();
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        assert_eq!(sign.back_text().get_message(0, false).as_ref(), "bcd");
        let (left, left_result) = execute(
            "data modify block 0 64 0 back_text.messages[1] set from block 1 64 0 front_text.messages[0]",
        );
        let (right, right_result) = execute(
            "data modify block 1 64 0 back_text.messages[1] set from block 0 64 0 front_text.messages[0]",
        );
        assert_eq!(
            left_result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(),
            1
        );
        assert_eq!(
            right_result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(),
            1
        );
        left.join().unwrap();
        right.join().unwrap();
        let before = fixture.block_data();
        let pending = fixture.pending_data();
        assert!(
            fixture
                .execute("data modify block 0 64 0 front_text.messages[0].bad set value 1")
                .is_err()
        );
        assert!(
            fixture
                .execute("data merge block 0 64 0 {is_waxed:0b}")
                .is_err()
        );
        assert_eq!(fixture.block_data(), before);
        assert_eq!(fixture.pending_data(), pending);
        assert!(!sign.mutation_is_locked());
        assert_eq!(saved(fixture.reload_saved_chunk().as_ref()), before);
    }
}

#[test]
fn sign_packet_text_keeps_selected_style_but_never_interprets_new_literal_contents() {
    let mut initial = compound(
        "{front_text:{messages:[{text:'raw',color:red,extra:['suffix']},'','',''],filtered_messages:[{text:'filtered',color:blue,bold:1b},'','','']},allow_op_features:1b}",
    );
    let attack =
        r#"{"text":"attack","click_event":{"action":"run_command","command":"op player"}}"#;
    for (allow, color, bold) in [(false, "blue", Some(true)), (true, "red", None)] {
        initial.put_bool("allow_op_features", allow);
        for hanging in [false, true] {
            let (_, live) = entity(hanging, &initial);
            let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
            let editor = uuid::Uuid::from_u128(75);
            *sign.currently_editing_player().lock().unwrap() = Some(editor);
            assert!(sign.update_player_text(
                editor,
                true,
                true,
                [attack, "", "", ""].map(Box::from)
            ));
            assert_eq!(sign.front_text().get_message(0, false).as_ref(), attack);
            assert_eq!(
                sign.front_text().get_messages(false),
                sign.front_text().get_messages(true)
            );
            assert_eq!(sign.click_commands(true, false), (allow, Vec::new()));
            let after = saved(live.as_ref());
            let line = after
                .get_compound("front_text")
                .unwrap()
                .get_list("messages")
                .unwrap()[0]
                .extract_compound()
                .unwrap();
            assert_eq!(line.get_string("color"), Some(color));
            assert_eq!(line.get_bool("bold"), bold);
            assert!(line.get("extra").is_none());
            assert_eq!(line.get_string("text"), Some(attack));
        }
    }
}

#[tokio::test]
async fn sign_privileged_reload_rebuilds_both_filtered_faces_from_raw_lines() {
    let fixture = Fixture::new().await;
    for hanging in [false, true] {
        let original = compound(
            "{front_text:{messages:[{text:'front',color:red},'','',''],filtered_messages:['filtered front','','','']},back_text:{messages:['back','','',''],filtered_messages:['filtered back','','','']}}",
        );
        let (block, live) = entity(hanging, &original);
        fixture.install(block, live.clone());
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        assert_eq!(
            sign.front_text().get_message(0, true).as_ref(),
            "filtered front"
        );
        assert_eq!(
            sign.back_text().get_message(0, true).as_ref(),
            "filtered back"
        );
        fixture
            .execute("data merge block 0 64 0 {allow_op_features:1b}")
            .unwrap();
        for face in [sign.front_text(), sign.back_text()] {
            assert_eq!(face.get_messages(true), face.get_messages(false));
        }
        let snapshot = fixture.block_data();
        assert_eq!(fixture.pending_data(), snapshot);
        assert_eq!(saved(fixture.reload_saved_chunk().as_ref()), snapshot);
        let mut privileged = original;
        privileged.put_bool("allow_op_features", true);
        let (_, loaded) = entity(hanging, &privileged);
        assert_eq!(saved(loaded.as_ref()), snapshot);
    }
}

#[test]
fn sign_invalid_click_command_is_preserved_on_load_and_refused_for_live_apply() {
    let invalid = compound(
        "{front_text:{messages:[{text:'bad',click_event:{action:'run_command',command:'say bad§text'}},'','','']},allow_op_features:1b}",
    );
    let (_, live) = entity(false, &invalid);
    let before = saved(live.as_ref());
    let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
    assert_eq!(sign.click_commands(true, false), (true, Vec::new()));
    assert!(live.load_nbt(&invalid).is_err());
    assert_eq!(saved(live.as_ref()), before);
    assert_eq!(
        SignData::from_nbt(&client_nbt_from_persisted(&before)).validate(),
        Ok(())
    );
}

/// Write physical compound-list entries directly: serializing NbtTag::List would
/// correctly add another wrapper around authored empty-key compounds, and would
/// therefore not exercise the raw transport boundary being checked here.
fn read_raw_message_compounds(entries: [NbtCompound; 4]) -> NbtCompound {
    use pumpkin_nbt::deserializer::{NbtReadHelperJava, NbtStreamReader};
    use pumpkin_nbt::serializer::{NbtWriteHelper, NbtWriteHelperJava};
    use pumpkin_nbt::{COMPOUND_ID, END_ID, LIST_ID};

    let mut bytes = Vec::new();
    {
        let mut writer = NbtWriteHelperJava::new(&mut bytes);
        writer.write_u8(COMPOUND_ID).unwrap(); // Unnamed root.
        writer.write_u8(COMPOUND_ID).unwrap();
        writer.write_string("front_text").unwrap();
        writer.write_u8(LIST_ID).unwrap();
        writer.write_string("messages").unwrap();
        writer.write_u8(COMPOUND_ID).unwrap();
        writer.write_i32(4).unwrap();
        for entry in entries {
            NbtTag::Compound(entry).serialize_data(&mut writer).unwrap();
        }
        writer.write_u8(END_ID).unwrap();
        writer.write_u8(END_ID).unwrap();
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let mut reader = NbtReadHelperJava::new(NbtStreamReader(&mut cursor));
    pumpkin_nbt::Nbt::read_unnamed(&mut reader)
        .unwrap()
        .root_tag
}

#[test]
fn sign_binary_mixed_list_wrappers_keep_all_four_line_positions() {
    let nbt = read_raw_message_compounds([
        compound("{text:'A',color:red}"),
        list_wrapper(NbtTag::String("".into())),
        list_wrapper(NbtTag::String("B".into())),
        compound("{text:'D'}"),
    ]);
    let lines = nbt
        .get_compound("front_text")
        .unwrap()
        .get_list("messages")
        .unwrap();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[1].extract_string(), Some(""));
    assert_eq!(lines[2].extract_string(), Some("B"));
    for hanging in [false, true] {
        let (_, live) = entity(hanging, &nbt);
        let sign = SignEntityRef::from_block_entity(live.as_ref()).unwrap();
        assert_eq!(
            sign.front_text().get_messages(false).map(String::from),
            ["A", "", "B", "D"]
        );
        assert_eq!(
            sign.front_text().get_messages(true),
            sign.front_text().get_messages(false)
        );
        assert!(live.load_nbt(&nbt).is_ok());
        // The 26.3 codec keeps plain lines as strings beside styled components.
        let roundtrip = SignData::from_nbt(&saved(live.as_ref()));
        assert!(roundtrip.validate().is_ok());
        assert_eq!(roundtrip.front.messages[1].text().as_ref(), "");
        assert_eq!(roundtrip.front.messages[2].text().as_ref(), "B");
    }
}

#[test]
fn sign_wrapper_decoding_is_one_level_at_binary_boundary_and_never_for_semantic_compounds() {
    let inner = list_wrapper(NbtTag::String("inner".into()));
    let mut multiple_fields = list_wrapper(NbtTag::String("literal".into()));
    multiple_fields.put_int("other", 1);
    let nbt = read_raw_message_compounds([
        compound("{text:'A',color:red}"),
        list_wrapper(NbtTag::Compound(inner.clone())),
        multiple_fields.clone(),
        compound("{text:'D'}"),
    ]);
    let lines = nbt
        .get_compound("front_text")
        .unwrap()
        .get_list("messages")
        .unwrap();
    assert_eq!(lines[1], NbtTag::Compound(inner.clone()));
    assert_eq!(lines[2], NbtTag::Compound(multiple_fields));
    let (_, live) = entity(false, &nbt);
    // Remaining semantic maps have no component content. Java skips them and
    // defaults this short face; peeling again would wrongly create literal text.
    assert!(live.load_nbt(&nbt).is_ok());
    let snapshot = saved(live.as_ref());
    let saved_lines = snapshot
        .get_compound("front_text")
        .unwrap()
        .get_list("messages")
        .unwrap();
    assert_eq!(saved_lines, &vec![NbtTag::String("".into()); 4]);
    assert_eq!(
        SignData::from_nbt(&client_nbt_from_persisted(&snapshot)).validate(),
        Ok(())
    );

    // Already decoded compounds are semantic values, not physical list wrappers.
    // Java's binary format permits empty keys; its SNBT parser rejects them.
    let mut face = NbtCompound::new();
    face.put_list(
        "messages",
        vec![
            NbtTag::String("A".into()),
            NbtTag::Compound(inner.clone()),
            NbtTag::String("C".into()),
            NbtTag::String("D".into()),
        ],
    );
    let mut authored = NbtCompound::new();
    authored.put_compound("front_text", face);
    assert_eq!(
        authored
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap()[1],
        NbtTag::Compound(inner)
    );
    assert!(SignData::from_nbt(&authored).validate().is_ok());
    let (_, authored_sign) = entity(false, &authored);
    assert!(authored_sign.load_nbt(&authored).is_ok());
    assert_eq!(
        SignEntityRef::from_block_entity(authored_sign.as_ref())
            .unwrap()
            .front_text()
            .get_message(2, false)
            .as_ref(),
        ""
    );
}

/// Build a physical list wrapper without routing an empty key through SNBT.
fn list_wrapper(value: NbtTag) -> NbtCompound {
    let mut compound = NbtCompound::new();
    compound.put("", value);
    compound
}

#[test]
fn sign_direct_flags_and_codec_booleans_match_java_numeric_conversions() {
    for (tag, direct, codec) in [
        (NbtTag::Byte(-128), true, true),
        (NbtTag::Short(256), false, true),
        (NbtTag::Int(257), true, true),
        (NbtTag::Long(i64::MIN), false, true),
        (NbtTag::Long(i64::MAX), true, true),
        (NbtTag::Float(0.5), false, true),
        (NbtTag::Double(-0.5), true, true),
        (NbtTag::Double(-0.0), false, false),
        (NbtTag::Float(f32::INFINITY), true, true),
        (NbtTag::Double(f64::NEG_INFINITY), false, true),
        (NbtTag::Double(f64::NAN), false, true),
    ] {
        let mut nbt = compound("{front_text:{messages:['A','B','C','D']}}");
        nbt.put("is_waxed", tag.clone());
        nbt.put("allow_op_features", tag.clone());
        let mut front = nbt.get_compound("front_text").unwrap().clone();
        front.put("has_glowing_text", tag);
        nbt.put_compound("front_text", front);
        let data = SignData::from_nbt(&nbt);
        assert_eq!(data.waxed, direct);
        assert_eq!(data.allow_op_features, direct);
        assert_eq!(data.front.has_glowing_text, codec);
        assert!(data.validate().is_ok());
    }
}

#[test]
fn sign_face_optional_errors_and_overlong_raw_lists_keep_java_partial_values() {
    for snbt in [
        "{messages:['A','B','C','D'],color:99,has_glowing_text:'bad'}",
        "{messages:['A','B','C','D'],color:invalid}",
        "{messages:['A','B','C','D','discarded']}",
        "{messages:['A',7,'B','C','D']}",
        "{messages:['A','B','C','D'],filtered_messages:['one','two','three','four','five']}",
        "{messages:['A','B','C','D'],filtered_messages:['one',7,'three','four']}",
    ] {
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(snbt))));
        assert_eq!(
            face.messages.map(|line| line.text().to_string()),
            ["A", "B", "C", "D"]
        );
        assert_eq!(
            face.filtered_messages.map(|line| line.text().to_string()),
            ["A", "B", "C", "D"]
        );
        assert_eq!(face.color, DyeColor::Black);
        assert!(!face.has_glowing_text);
    }
    let invalid = compound("{messages:['A',7,'C','D'],color:red,has_glowing_text:1b}");
    let face = FaceData::from_nbt(Some(&NbtTag::Compound(invalid)));
    assert_eq!(
        face.messages.map(|line| line.text().to_string()),
        ["", "", "", ""]
    );
    assert_eq!(face.color, DyeColor::Black);
    assert!(!face.has_glowing_text);
}

#[test]
fn sign_faces_skip_proven_invalid_components_and_keep_usable_list_partials() {
    for invalid in [
        "{}",
        "[]",
        "{color:red}",
        "{extra:['usable']}",
        "[17]",
        "[[]]",
        "{text:17}",
        "{text:['usable']}",
        "{score:{}}",
        "{type:17,text:'bad'}",
        "{type:unknown,text:'bad'}",
        "{type:text,translate:'key'}",
    ] {
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(&format!(
            "{{messages:['A',{invalid},'B','C','D']}}"
        )))));
        assert_eq!(
            face.messages.map(|line| line.text().to_string()),
            ["A", "B", "C", "D"],
            "{invalid}"
        );
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(&format!(
            "{{messages:['A',{invalid},'C','D'],color:red}}"
        )))));
        assert_eq!(
            face.messages.map(|line| line.text().to_string()),
            ["", "", "", ""],
            "{invalid}"
        );
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(&format!(
            "{{messages:['A','B','C','D'],filtered_messages:['one',{invalid},'three','four']}}"
        )))));
        assert_eq!(
            face.filtered_messages.map(|line| line.text().to_string()),
            ["A", "B", "C", "D"],
            "{invalid}"
        );
    }
    for partial in [
        "[17,'usable']",
        "['usable',17]",
        "[{},'usable']",
        "[[17,'usable']]",
    ] {
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(&format!(
            "{{messages:['A',{partial},'B','C','D']}}"
        )))));
        assert!(face.supported());
        assert_eq!(
            face.messages.map(|line| line.text().to_string()),
            ["A", "usable", "B", "C"],
            "{partial}"
        );
        let face = FaceData::from_nbt(Some(&NbtTag::Compound(compound(&format!(
            "{{messages:['A','B','C','D'],filtered_messages:['one',{partial},'three','four']}}"
        )))));
        assert!(face.supported());
        assert_eq!(
            face.filtered_messages.map(|line| line.text().to_string()),
            ["A", "B", "C", "D"],
            "{partial}"
        );
    }
}

#[tokio::test]
async fn sign_unknown_slot_decoding_preserves_entire_authored_faces_through_chunk_reload() {
    let fixture = Fixture::new().await;
    for unknown in [
        "{selector:'@a'}",
        "{text:17,translate:'key'}",
        "{sprite:'minecraft:foo'}",
        "{object:player,player:'Notch'}",
    ] {
        let original = compound(&format!(
            "{{front_text:{{messages:['A',{unknown},'B','C','D']}}}}"
        ));
        for hanging in [false, true] {
            let (block, live) = entity(hanging, &original);
            fixture.install(block, live.clone());
            let before = fixture.block_data();
            let authored = original
                .get_compound("front_text")
                .unwrap()
                .get_list("messages")
                .unwrap();
            assert_eq!(
                before
                    .get_compound("front_text")
                    .unwrap()
                    .get_list("messages"),
                Some(authored)
            );
            assert!(
                fixture
                    .execute("data merge block 0 64 0 {is_waxed:1b}")
                    .is_err()
            );
            assert_eq!(fixture.block_data(), before);
            assert_eq!(fixture.pending_data(), before);
            assert_eq!(saved(fixture.reload_saved_chunk().as_ref()), before);
            let client = client_nbt_from_persisted(&before);
            assert!(SignData::from_nbt(&client).validate().is_ok());
        }
    }
}
