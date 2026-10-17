/** Bounded ordinary-item crafting probe, opt-in only. Primary: exact 26.3
 * CraftingPackets a8888ec1… and CraftingWireCodecs cffefd6b…; the latter's
 * original failed postvalidator receipt remains failed (separate validation).
 * These helpers never predict recipes or mutate received inventory snapshots. */
import { plainStackHash, readVarInt, varInt, type ContainerPacket, type PlainStack } from './client.ts';

export type CraftAction =
  | { input: 'pickup' | 'quick_move' | 'throw'; button: 0 | 1 }
  | { input: 'swap'; button: number };
const actionIds = { pickup: 0, quick_move: 1, swap: 2, throw: 4 } as const;

export function craftingAction(input: unknown, button: unknown): CraftAction {
  if (typeof button !== 'number' || !Number.isInteger(button)) throw new Error('Invalid crafting button');
  if (input === 'swap' && button >= 0 && button <= 8) return { input, button };
  if ((input === 'pickup' || input === 'quick_move' || input === 'throw') && (button === 0 || button === 1)) return { input, button };
  throw new Error('Unsupported crafting action/button');
}

export type CraftMenu = {
  container_id: number; kind: 'player' | 'table'; state_id: number | null;
  revision_sequence: number | null; baseline_sequence: number | null;
  slots: PlainStack[] | null;
};
export type CraftingState = {
  player: CraftMenu | null; table: CraftMenu | null; active: 'player' | 'table';
  cursor: PlainStack; cursor_sequence: number | null;
  received_cursors: Record<'player' | 'table', { stack: PlainStack; sequence: number | null }>;
};
export type CraftingPacket = ContainerPacket
  | { kind: 'cursor'; packet_id: 98; stack: PlainStack }
  | { kind: 'player_slot'; packet_id: 110; slot: number; stack: PlainStack };

export function emptyCraftingState(): CraftingState {
  return { player: null, table: null, active: 'player', cursor: null, cursor_sequence: null,
    received_cursors: { player: { stack: null, sequence: null }, table: { stack: null, sequence: null } } };
}

/** Expose only the active menu's received cursor. A null sequence is unknown,
 * distinct from an empty stack received in content18 or cursor98. */
function activeCraftingCursor(state: CraftingState): CraftingState {
  const cursor = state.received_cursors[state.active];
  return { ...state, cursor: cursor.stack, cursor_sequence: cursor.sequence };
}

/** Inventory indexes are not menu indexes: offhand40 never projects to a table. */
export function craftingInventorySlot(kind: 'player' | 'table', inventory: number): number | null {
  if (!Number.isInteger(inventory) || inventory < 0 || inventory > 40) throw new Error('Invalid player inventory index');
  if (inventory <= 8) return (kind === 'player' ? 36 : 37) + inventory;
  if (inventory <= 35) return inventory + (kind === 'player' ? 0 : 1);
  if (kind === 'table') return null;
  return inventory === 40 ? 45 : 44 - inventory; // inventory36..39 boots..helmet.
}

function copyStack(stack: PlainStack): PlainStack {
  plainStackHash(stack); // Refuse component-bearing observations in this mode.
  return stack === null ? null : { item_id: stack.item_id, count: stack.count };
}

/** Decode only the two independent ordinary-stack packets missing from the
 * existing menu codec. Count-first ItemStack is distinct from item-first hash. */
export function decodeCraftingPacket(payload: Buffer): CraftingPacket {
  if (payload.length > 128) throw new Error('Crafting standalone packet budget exceeded');
  let offset = 0;
  const integer = () => {
    const result = readVarInt(payload, offset);
    if (!result) throw new Error('Truncated crafting VarInt');
    offset += result.bytes;
    return result.value;
  };
  const packet_id = integer();
  if (packet_id !== 98 && packet_id !== 110) throw new Error('Expected cursor98 or player inventory110');
  const slot = packet_id === 110 ? integer() : null;
  if (slot !== null) craftingInventorySlot('player', slot);
  const count = integer();
  if (count < 0) throw new Error('Negative plain item count');
  let stack: PlainStack = null;
  if (count > 0) {
    const item_id = integer(), added = integer(), removed = integer();
    if (item_id <= 0 || added !== 0 || removed !== 0) throw new Error('Unsupported crafting item/component patch');
    stack = { item_id, count };
  }
  if (offset !== payload.length) throw new Error('Trailing crafting packet bytes');
  return packet_id === 98 ? { kind: 'cursor', packet_id, stack }
    : { kind: 'player_slot', packet_id, slot: slot!, stack };
}

function projectInventory(menu: CraftMenu | null, inventory: number, stack: PlainStack): CraftMenu | null {
  if (menu?.slots == null) return menu; // Deltas cannot manufacture full content.
  const slot = craftingInventorySlot(menu.kind, inventory);
  if (slot === null) return menu;
  const slots = menu.slots.slice();
  slots[slot] = copyStack(stack);
  return { ...menu, slots }; // Independent updates never advance menu revision.
}

/** Derive received views, keeping the raw packets separately in the client.
 * Only actual content establishes a 46-slot baseline; only a matching menu
 * packet advances its revision. Shared player slots project across baselines. */
export function observeCrafting(current: CraftingState, packet: CraftingPacket & { sequence: number }): CraftingState {
  const next = { ...current };
  if (packet.kind === 'cursor') return activeCraftingCursor({ ...next, received_cursors: { ...next.received_cursors,
    [next.active]: { stack: copyStack(packet.stack), sequence: packet.sequence } } });
  if (packet.kind === 'player_slot') return { ...next,
    player: projectInventory(next.player, packet.slot, packet.stack),
    table: projectInventory(next.table, packet.slot, packet.stack) };
  if (packet.kind === 'open') {
    if (packet.container_id <= 0 || packet.menu_type !== 12) throw new Error('Crafting requires an actual table type12');
    return activeCraftingCursor({ ...next, active: 'table', table: { kind: 'table', container_id: packet.container_id,
      state_id: null, revision_sequence: null, baseline_sequence: null, slots: null },
      received_cursors: { ...next.received_cursors, table: { stack: null, sequence: null } } });
  }
  const key = packet.container_id === 0 ? 'player' : 'table';
  let menu = next[key];
  if (key === 'table' && menu?.container_id !== packet.container_id) throw new Error('Unopened crafting table update');
  if (packet.kind === 'content') {
    if (packet.slots.length !== 46) throw new Error('Crafting requires exactly 46 slots');
    menu = { kind: key, container_id: packet.container_id, state_id: packet.state_id,
      baseline_sequence: packet.sequence, revision_sequence: packet.sequence, slots: packet.slots.map(copyStack) };
    next.received_cursors = { ...next.received_cursors,
      [key]: { stack: copyStack(packet.carried), sequence: packet.sequence } };
  } else {
    if (packet.slot < 0 || packet.slot >= 46) throw new Error('Crafting slot outside menu');
    if (menu?.slots == null) return next;
    const slots = menu.slots.slice();
    slots[packet.slot] = copyStack(packet.stack);
    menu = { ...menu, state_id: packet.state_id, revision_sequence: packet.sequence, slots };
  }
  next[key] = menu;
  const other = key === 'player' ? 'table' : 'player';
  // In particular, table slot40 is hotbar3; its inventory index is3, not40.
  for (let inventory = 0; inventory <= 40; inventory++) {
    const slot = craftingInventorySlot(key, inventory);
    if (slot !== null && (packet.kind === 'content' || packet.slot === slot)) {
      next[other] = projectInventory(next[other], inventory, menu.slots![slot]!);
    }
  }
  return activeCraftingCursor(next);
}

/** A local close selects the retained player baseline and received cursor.
 * It does not predict an empty cursor or evacuated grid contents. */
export function closeCrafting(state: CraftingState): CraftingState {
  return activeCraftingCursor({ ...state, active: 'player', table: null,
    received_cursors: { ...state.received_cursors, table: { stack: null, sequence: null } } });
}

export function encodeCraftingClick(state: CraftingState, slot: number, action: CraftAction): Buffer {
  const menu = state[state.active];
  const checked = craftingAction(action.input, action.button);
  if (!menu || menu.slots?.length !== 46 || menu.state_id === null || state.cursor_sequence === null ||
      !Number.isInteger(menu.container_id) || menu.container_id < 0 || menu.container_id > 255 ||
      (menu.kind === 'player' ? menu.container_id !== 0 : menu.container_id === 0) ||
      !Number.isInteger(menu.state_id) || menu.state_id < 0 || menu.state_id > 32767 ||
      !Number.isInteger(slot) || slot < 0 || slot >= 46) throw new Error('Crafting click requires a synchronized received menu');
  const index = Buffer.alloc(2);
  index.writeInt16BE(slot);
  return Buffer.concat([varInt(menu.container_id), varInt(menu.state_id), index,
    Buffer.from([checked.button]), varInt(actionIds[checked.input]), varInt(0), plainStackHash(state.cursor)]);
}
