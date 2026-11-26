package dev.ferraloom.bukkit;

/**
 * The inventory area's calls into the server (src/inventory.rs). An inventory is named by
 * {@code (kind, a, b)}: 0 a player's inventory, 1 their ender chest, 2 their crafting grid (a and b
 * their UUID's halves); 3 a plugin's container (a its id); 4 a block's (a the world, b the
 * position as BlockPos.asLong); 5 the container of the menu a player has open. Items are
 * {@link Items}' bytes, null for none. Server thread only, but {@link #dropContainer}.
 */
final class InventoryNative {
    private InventoryNative() {
    }

    /** The inventory's slot count, or -1 when it is gone. */
    static native int size(int kind, long a, long b);

    static native byte[] slot(int kind, long a, long b, int slot);

    /** False when there is no such slot. */
    static native boolean setSlot(int kind, long a, long b, int slot, byte[] item);

    /** Sets the slot only while it still holds {@code expected}: an item mirror's write. */
    static native boolean replaceSlot(int kind, long a, long b, int slot, byte[] expected, byte[] item);

    static native int heldSlot(long most, long least);

    static native boolean setHeldSlot(long most, long least, int slot);

    static native byte[] cursor(long most, long least);

    static native boolean setCursor(long most, long least, byte[] item);

    /** A new plugin container of {@code size} empty slots: its id. */
    static native int createContainer(int size);

    /** Frees a plugin container; from any thread (its object's cleaner). */
    static native void dropContainer(int id);

    /**
     * Shows the inventory to the player in a {@code menu} ({@code minecraft:generic_9x3}) titled {@code title} (a text
     * component's NBT). A {@code retitle} shows the open menu again: nothing closes and the cursor stays.
     */
    static native boolean open(long most, long least, int kind, long a, long b, String menu, byte[] title, boolean retitle);

    /** Closes the player's open menu; false when none was open. */
    static native boolean close(long most, long least);

    /** Sends the player's inventory and open menu again. */
    static native void resend(long most, long least);

    /** An item's {max stack, max damage, edible, fuel, record, compostable}, or null for an unknown item. */
    static native int[] itemFacts(String item);

    /** The item crafting leaves of it (a bucket's bucket), or null. */
    static native String itemRemainder(String item);
}
