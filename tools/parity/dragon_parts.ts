/** Source-derived exact26.3 codecs, not runtime packet gold. Packet primary
 * 279429a1…; collection leaf d03bd067… and readCount 6752df8e….
 * LpVec3's body is NOT retained: spawn decoding deliberately stops before it. */
import { createHash } from 'node:crypto';
import { readVarInt, varInt } from './client.ts';

export const DRAGON_LIMITS = { packet_bytes: 65536, removal_ids: 4096,
  evidence_bytes: 8 * 1024 * 1024, events: 4096, attacks: 32 } as const;

type RawPacket = { payload_base64: string; payload_sha256: string; payload_bytes: number; decoded_bytes: number };
export type DragonPacket = RawPacket & (
  | { kind: 'spawn_header'; packet_id: 1; entity_id: number; uuid: string; entity_type: number;
      x: number; y: number; z: number; complete: false; undecoded_remainder_base64: string }
  | { kind: 'remove'; packet_id: 78; entity_ids: number[]; complete: true }
);
export type DragonObservation = DragonPacket & { sequence: number };

/** Latest Attack is one signed VAR_INT, with no historical interact subaction. */
export function encodeDragonAttack(entityId: number): Buffer {
  if (!Number.isInteger(entityId) || entityId < -2147483648 || entityId > 2147483647) {
    throw new Error('Dragon Attack requires a signed i32 entity ID');
  }
  return varInt(entityId);
}

export function decodeDragonPacket(payload: Buffer): DragonPacket {
  if (payload.length > DRAGON_LIMITS.packet_bytes) throw new Error('Dragon packet budget exceeded');
  let offset = 0;
  const integer = () => {
    const read = readVarInt(payload, offset);
    if (!read) throw new Error('Truncated Dragon VarInt');
    offset += read.bytes;
    return read.value;
  };
  const take = (size: number) => {
    if (size > payload.length - offset) throw new Error('Truncated Dragon spawn header');
    const bytes = payload.subarray(offset, offset + size);
    offset += size;
    return bytes;
  };
  const packet_id = integer();
  const raw = { payload_base64: payload.toString('base64'),
    payload_sha256: createHash('sha256').update(payload).digest('hex'), payload_bytes: payload.length };
  if (packet_id === 1) {
    const entity_id = integer();
    const hex = take(16).toString('hex');
    const uuid = `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
    const entity_type = integer();
    if (entity_type < 0) throw new Error('Negative Dragon spawn registry ID');
    const x = take(8).readDoubleBE(), y = take(8).readDoubleBE(), z = take(8).readDoubleBE();
    if (![x, y, z].every(Number.isFinite)) throw new Error('Nonfinite Dragon spawn position');
    if (offset === payload.length) throw new Error('Missing undecoded Dragon spawn remainder');
    return { ...raw, kind: 'spawn_header', packet_id, entity_id, uuid, entity_type, x, y, z,
      decoded_bytes: offset, complete: false, undecoded_remainder_base64: payload.subarray(offset).toString('base64') };
  }
  if (packet_id !== 78) throw new Error('Expected AddEntity1 or RemoveEntities78');
  // ByteBufCodecs$26: readCount(VAR_INT), then exactly count VAR_INTs.
  // This probe's smaller cap is a capture budget, not a claimed wire maximum.
  const count = integer();
  if (count < 0 || count > DRAGON_LIMITS.removal_ids) throw new Error('Dragon removal count exceeds probe budget');
  if (count > payload.length - offset) throw new Error('Truncated Dragon removal IDs');
  const entity_ids = Array.from({ length: count }, integer);
  if (offset !== payload.length) throw new Error('Trailing Dragon removal bytes');
  return { ...raw, kind: 'remove', packet_id, entity_ids, complete: true, decoded_bytes: offset };
}

/** Never evict evidence: the whole probe fails when a capture cap is exceeded. */
export class DragonBudget {
  bytes = 0;
  events = 0;
  attacks = 0;
  retain(bytes: number): void {
    this.bytes += bytes;
    this.events++;
    if (this.bytes > DRAGON_LIMITS.evidence_bytes || this.events > DRAGON_LIMITS.events) {
      throw new Error('Dragon evidence budget exceeded');
    }
  }
  attack(): number {
    if (++this.attacks > DRAGON_LIMITS.attacks) throw new Error('Dragon Attack budget exceeded');
    return this.attacks;
  }
}
