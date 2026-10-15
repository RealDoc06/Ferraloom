#[allow(clippy::wildcard_imports)]
use super::*;

// Java 26.3 ignores superseded IDs before validating their payload. The current
// ID still requires an outstanding destination and a valid movement echo.
fn confirmed_destination(
    pending: Option<&(VarInt, Vector3<f64>)>,
    last_issued_id: i32,
    packet: &SConfirmTeleport,
) -> Result<Option<Vector3<f64>>, ()> {
    let current_id = pending.map_or(last_issued_id, |(id, _)| id.0);
    if packet.teleport_id.0 != current_id {
        return Ok(None);
    }
    let Some((_, destination)) = pending else {
        return Err(());
    };
    if packet.position.x.is_nan()
        || packet.position.y.is_nan()
        || packet.position.z.is_nan()
        || !packet.yaw.is_finite()
        || !packet.pitch.is_finite()
    {
        return Err(());
    }
    Ok(Some(*destination))
}

impl JavaClient {
    pub fn handle_confirm_teleport(&self, player: &Player, confirm_teleport: &SConfirmTeleport) {
        let result = {
            let mut awaiting_teleport = player
                .awaiting_teleport
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let result = confirmed_destination(
                awaiting_teleport.as_ref(),
                player.teleport_id_count.load(Ordering::Relaxed),
                confirm_teleport,
            );
            if let Ok(Some(destination)) = result {
                player.get_entity().set_pos(destination);
                *awaiting_teleport = None;
            }
            result
        };

        if result.is_err() {
            self.try_kick(&TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                translation::java::MULTIPLAYER_DISCONNECT_INVALID_PLAYER_MOVEMENT,
                [],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(id: i32) -> SConfirmTeleport {
        SConfirmTeleport {
            teleport_id: id.into(),
            position: Vector3::new(4.5, 200.0, -3.5),
            yaw: 180.0,
            pitch: 0.0,
        }
    }

    #[test]
    fn overlapping_teleports_ignore_old_and_future_acknowledgements() {
        let destination = Vector3::new(4097.5, 200.0, 3.5);
        let pending = (VarInt(2), destination);
        for id in [1, 3, i32::MIN, i32::MAX] {
            assert_eq!(
                confirmed_destination(Some(&pending), 2, &packet(id)),
                Ok(None)
            );
        }
        assert_eq!(
            confirmed_destination(Some(&pending), 2, &packet(2)),
            Ok(Some(destination))
        );
        assert_eq!(confirmed_destination(None, 2, &packet(1)), Ok(None));
        assert_eq!(confirmed_destination(None, 2, &packet(2)), Err(()));
    }

    #[test]
    fn matching_acknowledgement_validates_nan_coordinates_and_nonfinite_angles() {
        let pending = (VarInt(2), Vector3::new(1.0, 2.0, 3.0));
        for axis in 0..5 {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut acknowledgement = packet(2);
                match axis {
                    0 => acknowledgement.position.x = invalid,
                    1 => acknowledgement.position.y = invalid,
                    2 => acknowledgement.position.z = invalid,
                    3 => acknowledgement.yaw = invalid as f32,
                    _ => acknowledgement.pitch = invalid as f32,
                }
                // Java clamps infinite coordinates later, but rejects NaN
                // coordinates and every nonfinite angle at this boundary.
                let invalid_here = axis >= 3 || invalid.is_nan();
                assert_eq!(
                    confirmed_destination(Some(&pending), 2, &acknowledgement).is_err(),
                    invalid_here
                );
                acknowledgement.teleport_id = 1.into();
                assert_eq!(
                    confirmed_destination(Some(&pending), 2, &acknowledgement),
                    Ok(None)
                );
            }
        }
    }
}
