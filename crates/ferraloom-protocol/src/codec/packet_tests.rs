//! Every pre-play packet ID is exercised against vanilla's ProtocolInfo codec.
use super::*;
fn hex(text: &str) -> Vec<u8> {
    text.trim()
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn vanilla_handshake_sb() {
    for fixture in [include_str!(
        "../../tests/fixtures/handshake_sb_intention.hex"
    )] {
        let bytes = hex(fixture);
        let packet = crate::handshake::ServerboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_status_sb() {
    for fixture in [
        include_str!("../../tests/fixtures/status_sb_ping.hex"),
        include_str!("../../tests/fixtures/status_sb_request.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::status::ServerboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_status_cb() {
    for fixture in [
        include_str!("../../tests/fixtures/status_cb_pong.hex"),
        include_str!("../../tests/fixtures/status_cb_response.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::status::ClientboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_login_sb() {
    for fixture in [
        include_str!("../../tests/fixtures/login_sb_ack.hex"),
        include_str!("../../tests/fixtures/login_sb_cookie.hex"),
        include_str!("../../tests/fixtures/login_sb_hello.hex"),
        include_str!("../../tests/fixtures/login_sb_key.hex"),
        include_str!("../../tests/fixtures/login_sb_query.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::login::ServerboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_login_cb() {
    for fixture in [
        include_str!("../../tests/fixtures/login_cb_compression.hex"),
        include_str!("../../tests/fixtures/login_cb_cookie.hex"),
        include_str!("../../tests/fixtures/login_cb_disconnect.hex"),
        include_str!("../../tests/fixtures/login_cb_finished.hex"),
        include_str!("../../tests/fixtures/login_cb_hello.hex"),
        include_str!("../../tests/fixtures/login_cb_query.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::login::ClientboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_config_sb() {
    for fixture in [
        include_str!("../../tests/fixtures/config_sb_click.hex"),
        include_str!("../../tests/fixtures/config_sb_conduct.hex"),
        include_str!("../../tests/fixtures/config_sb_cookie.hex"),
        include_str!("../../tests/fixtures/config_sb_finish.hex"),
        include_str!("../../tests/fixtures/config_sb_information.hex"),
        include_str!("../../tests/fixtures/config_sb_keepalive.hex"),
        include_str!("../../tests/fixtures/config_sb_knownpacks.hex"),
        include_str!("../../tests/fixtures/config_sb_pack.hex"),
        include_str!("../../tests/fixtures/config_sb_payload.hex"),
        include_str!("../../tests/fixtures/config_sb_pong.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::configuration::ServerboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn vanilla_config_cb() {
    for fixture in [
        include_str!("../../tests/fixtures/config_cb_clear.hex"),
        include_str!("../../tests/fixtures/config_cb_conduct.hex"),
        include_str!("../../tests/fixtures/config_cb_cookie.hex"),
        include_str!("../../tests/fixtures/config_cb_dialog.hex"),
        include_str!("../../tests/fixtures/config_cb_disconnect.hex"),
        include_str!("../../tests/fixtures/config_cb_effects.hex"),
        include_str!("../../tests/fixtures/config_cb_features.hex"),
        include_str!("../../tests/fixtures/config_cb_finish.hex"),
        include_str!("../../tests/fixtures/config_cb_keepalive.hex"),
        include_str!("../../tests/fixtures/config_cb_knownpacks.hex"),
        include_str!("../../tests/fixtures/config_cb_links.hex"),
        include_str!("../../tests/fixtures/config_cb_payload.hex"),
        include_str!("../../tests/fixtures/config_cb_ping.hex"),
        include_str!("../../tests/fixtures/config_cb_pop.hex"),
        include_str!("../../tests/fixtures/config_cb_push.hex"),
        include_str!("../../tests/fixtures/config_cb_registry.hex"),
        include_str!("../../tests/fixtures/config_cb_report.hex"),
        include_str!("../../tests/fixtures/config_cb_reset.hex"),
        include_str!("../../tests/fixtures/config_cb_store.hex"),
        include_str!("../../tests/fixtures/config_cb_tags.hex"),
        include_str!("../../tests/fixtures/config_cb_transfer.hex"),
    ] {
        let bytes = hex(fixture);
        let packet = crate::configuration::ClientboundPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.to_bytes().unwrap(), bytes, "{packet:?}");
    }
}

#[test]
fn query_answer_accepts_vanillas_undiscriminated_payload() {
    let bytes = [2, 42];
    let packet = crate::login::ServerboundPacket::from_bytes(&bytes).unwrap();
    assert_eq!(packet.to_bytes().unwrap(), bytes);
}
#[test]
fn explicit_packet_limits_and_nbt_budgets() {
    let mut w = Writer::new();
    w.count(65, 100).unwrap();
    assert!(crate::configuration::ServerboundSelectKnownPacks::from_bytes(w.as_slice()).is_err());
    let mut payload = Writer::new();
    payload.extend(&[7]);
    payload.i32(32769);
    let mut r = Reader::new(payload.as_slice());
    assert!(
        r.nbt_with_accounter(&mut ferraloom_nbt::NbtAccounter::new(32768, 16))
            .is_err()
    );
    let invalid: Vec<_> = [7].into_iter().chain((2097152i32).to_be_bytes()).collect();
    assert!(Reader::new(&invalid).nbt(false).is_err());
}
