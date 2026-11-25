package dev.ferraloom.bukkit;

import com.google.common.base.Preconditions;
import dev.ferraloom.bukkit.stub.StubInventory;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.ListIterator;
import java.util.NoSuchElementException;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/**
 * CraftInventory: an inventory's slots, held by the server and named by {@code (kind, a, b)}
 * ({@link InventoryNative}). Nothing is kept here: each read asks the server, and an item read
 * from a slot is a mirror of it ({@link FerraloomItemStack}). The searches and moves
 * ({@link #addItem}, {@link #removeItem}, {@link #first}...) are CraftInventory's, over
 * {@link #getItem} and {@link #setItem}.
 */
class FerraloomInventory extends StubInventory {
    final int kind;
    final long a;
    final long b;
    final InventoryType type;
    /** The slot count when it is fixed (a player's, a plugin's), else -1: asked each time. */
    private final int fixedSize;
    /** The holder a plugin gave (createInventory), or the player. */
    private final InventoryHolder holder;
    /** CraftInventoryCustom's title, as the plugin gave it. */
    final String title;
    private int maxStackSize = 99;

    FerraloomInventory(int kind, long a, long b, InventoryType type, int fixedSize, InventoryHolder holder, String title) {
        this.kind = kind;
        this.a = a;
        this.b = b;
        this.type = type;
        this.fixedSize = fixedSize;
        this.holder = holder;
        this.title = title == null ? type.getDefaultTitle() : title;
    }

    /** Whether this is the inventory {@code (kind, a, b)} names. */
    boolean is(int kind, long a, long b) {
        return this.kind == kind && this.a == a && this.b == b;
    }

    /** The {@code minecraft:menu} that shows this inventory, or null for one no container menu shows. */
    String menu() {
        int size = getSize();
        return switch (type) {
            case CHEST, ENDER_CHEST, BARREL -> size % 9 == 0 && size >= 9 && size <= 54 ? "minecraft:generic_9x" + size / 9 : null;
            case DISPENSER, DROPPER -> "minecraft:generic_3x3";
            case HOPPER -> "minecraft:hopper";
            case SHULKER_BOX -> "minecraft:shulker_box";
            default -> null;
        };
    }

    @Override
    public int getSize() {
        if (fixedSize >= 0) {
            return fixedSize;
        }
        return Math.max(0, InventoryNative.size(kind, a, b));
    }

    @Override
    public int getMaxStackSize() {
        return maxStackSize;
    }

    @Override
    public void setMaxStackSize(int size) {
        maxStackSize = size;
    }

    @Override
    public ItemStack getItem(int index) {
        byte[] bytes = InventoryNative.slot(kind, a, b, index);
        ItemStack read = Items.read(bytes);
        return read == null ? null : new FerraloomItemStack(this, index, bytes, read);
    }

    @Override
    public void setItem(int index, ItemStack item) {
        InventoryNative.setSlot(kind, a, b, index, Items.write(item));
    }

    @Override
    public ItemStack[] getContents() {
        ItemStack[] items = new ItemStack[getSize()];
        for (int i = 0; i < items.length; i++) {
            items[i] = getItem(i);
        }
        return items;
    }

    @Override
    public void setContents(ItemStack[] items) {
        int size = getSize();
        Preconditions.checkArgument(items.length <= size, "Invalid inventory size (%s); expected %s or less", items.length, size);
        for (int i = 0; i < size; i++) {
            setItem(i, i < items.length ? items[i] : null);
        }
    }

    @Override
    public ItemStack[] getStorageContents() {
        return getContents();
    }

    @Override
    public void setStorageContents(ItemStack[] items) {
        setContents(items);
    }

    @Override
    public boolean contains(Material material) {
        Preconditions.checkArgument(material != null, "Material cannot be null");
        for (ItemStack item : getStorageContents()) {
            if (item != null && item.getType() == material) {
                return true;
            }
        }
        return false;
    }

    @Override
    public boolean contains(ItemStack item) {
        if (item == null) {
            return false;
        }
        for (ItemStack i : getStorageContents()) {
            if (item.equals(i)) {
                return true;
            }
        }
        return false;
    }

    @Override
    public boolean contains(Material material, int amount) {
        Preconditions.checkArgument(material != null, "Material cannot be null");
        if (amount <= 0) {
            return true;
        }
        for (ItemStack item : getStorageContents()) {
            if (item != null && item.getType() == material && (amount -= item.getAmount()) <= 0) {
                return true;
            }
        }
        return false;
    }

    @Override
    public boolean contains(ItemStack item, int amount) {
        if (item == null) {
            return false;
        }
        if (amount <= 0) {
            return true;
        }
        for (ItemStack i : getStorageContents()) {
            if (item.equals(i) && --amount <= 0) {
                return true;
            }
        }
        return false;
    }

    @Override
    public boolean containsAtLeast(ItemStack item, int amount) {
        if (item == null) {
            return false;
        }
        if (amount <= 0) {
            return true;
        }
        for (ItemStack i : getStorageContents()) {
            if (item.isSimilar(i) && (amount -= i.getAmount()) <= 0) {
                return true;
            }
        }
        return false;
    }

    @Override
    public HashMap<Integer, ItemStack> all(Material material) {
        Preconditions.checkArgument(material != null, "Material cannot be null");
        HashMap<Integer, ItemStack> slots = new HashMap<>();
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] != null && items[i].getType() == material) {
                slots.put(i, items[i]);
            }
        }
        return slots;
    }

    @Override
    public HashMap<Integer, ItemStack> all(ItemStack item) {
        HashMap<Integer, ItemStack> slots = new HashMap<>();
        if (item != null) {
            ItemStack[] items = getStorageContents();
            for (int i = 0; i < items.length; i++) {
                if (item.equals(items[i])) {
                    slots.put(i, items[i]);
                }
            }
        }
        return slots;
    }

    @Override
    public int first(Material material) {
        Preconditions.checkArgument(material != null, "Material cannot be null");
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] != null && items[i].getType() == material) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public int first(ItemStack item) {
        return first(item, true);
    }

    private int first(ItemStack item, boolean withAmount) {
        if (item == null) {
            return -1;
        }
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] != null && (withAmount ? item.equals(items[i]) : item.isSimilar(items[i]))) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public int firstEmpty() {
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] == null) {
                return i;
            }
        }
        return -1;
    }

    @Override
    public boolean isEmpty() {
        for (ItemStack item : getContents()) {
            if (item != null) {
                return false;
            }
        }
        return true;
    }

    private int firstPartial(ItemStack item) {
        if (item == null) {
            return -1;
        }
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            ItemStack c = items[i];
            if (c != null && c.getAmount() < maxStack(c) && c.isSimilar(item)) {
                return i;
            }
        }
        return -1;
    }

    /** CraftInventory.getMaxItemStack: the item's own limit, at most this inventory's. */
    private int maxStack(ItemStack item) {
        return Math.min(item.getMaxStackSize(), getMaxStackSize());
    }

    /** CraftInventory.addItem: fills partial stacks, then empty slots; what did not fit, by argument index. */
    @Override
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        Preconditions.checkArgument(items != null, "items cannot be null");
        HashMap<Integer, ItemStack> leftover = new HashMap<>();
        for (int i = 0; i < items.length; i++) {
            ItemStack item = items[i];
            Preconditions.checkArgument(item != null, "ItemStack cannot be null");
            while (true) {
                int partial = firstPartial(item);
                if (partial == -1) {
                    int free = firstEmpty();
                    if (free == -1) {
                        leftover.put(i, item);
                        break;
                    }
                    int max = maxStack(item);
                    if (item.getAmount() > max) {
                        ItemStack stack = item.clone();
                        stack.setAmount(max);
                        setItem(free, stack);
                        item.setAmount(item.getAmount() - max);
                    } else {
                        setItem(free, item);
                        break;
                    }
                } else {
                    ItemStack partialItem = getItem(partial);
                    int amount = item.getAmount();
                    int partialAmount = partialItem.getAmount();
                    int max = maxStack(partialItem);
                    if (amount + partialAmount <= max) {
                        partialItem.setAmount(amount + partialAmount);
                        setItem(partial, partialItem);
                        break;
                    }
                    partialItem.setAmount(max);
                    setItem(partial, partialItem);
                    item.setAmount(amount + partialAmount - max);
                }
            }
        }
        return leftover;
    }

    /** CraftInventory.removeItem: takes similar stacks until each amount is gone; what was missing, by argument index. */
    @Override
    public HashMap<Integer, ItemStack> removeItem(ItemStack... items) {
        Preconditions.checkArgument(items != null, "items cannot be null");
        HashMap<Integer, ItemStack> leftover = new HashMap<>();
        for (int i = 0; i < items.length; i++) {
            ItemStack item = items[i];
            Preconditions.checkArgument(item != null, "ItemStack cannot be null");
            int toDelete = item.getAmount();
            while (true) {
                int first = first(item, false);
                if (first == -1) {
                    item.setAmount(toDelete);
                    leftover.put(i, item);
                    break;
                }
                ItemStack stack = getItem(first);
                int amount = stack.getAmount();
                if (amount <= toDelete) {
                    toDelete -= amount;
                    clear(first);
                } else {
                    stack.setAmount(amount - toDelete);
                    setItem(first, stack);
                    toDelete = 0;
                }
                if (toDelete <= 0) {
                    break;
                }
            }
        }
        return leftover;
    }

    @Override
    public void remove(Material material) {
        Preconditions.checkArgument(material != null, "Material cannot be null");
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] != null && items[i].getType() == material) {
                clear(i);
            }
        }
    }

    @Override
    public void remove(ItemStack item) {
        ItemStack[] items = getStorageContents();
        for (int i = 0; i < items.length; i++) {
            if (items[i] != null && items[i].equals(item)) {
                clear(i);
            }
        }
    }

    @Override
    public void clear(int index) {
        setItem(index, null);
    }

    @Override
    public void clear() {
        for (int i = 0; i < getSize(); i++) {
            clear(i);
        }
    }

    /** The online players who have this inventory open. */
    @Override
    public List<HumanEntity> getViewers() {
        List<HumanEntity> viewers = new ArrayList<>();
        for (org.bukkit.entity.Player p : Bridge.server.getOnlinePlayers()) {
            FerraloomInventoryView open = p instanceof FerraloomPlayer f ? f.inventories.open() : null;
            if (open != null && open.top == this) {
                viewers.add(p);
            }
        }
        return viewers;
    }

    @Override
    public InventoryType getType() {
        return type;
    }

    @Override
    public InventoryHolder getHolder() {
        return holder;
    }

    @Override
    public ListIterator<ItemStack> iterator() {
        return iterator(0);
    }

    /** CraftBukkit's InventoryIterator: a list iterator over the slots; {@code set} sets the slot. */
    @Override
    public ListIterator<ItemStack> iterator(int index) {
        int start = index < 0 ? getSize() + index + 1 : index;
        return new ListIterator<>() {
            private int next = start;
            private int last = -1;

            @Override
            public boolean hasNext() {
                return next < getSize();
            }

            @Override
            public ItemStack next() {
                if (!hasNext()) {
                    throw new NoSuchElementException();
                }
                last = next;
                return getItem(next++);
            }

            @Override
            public boolean hasPrevious() {
                return next > 0;
            }

            @Override
            public ItemStack previous() {
                if (!hasPrevious()) {
                    throw new NoSuchElementException();
                }
                last = --next;
                return getItem(next);
            }

            @Override
            public int nextIndex() {
                return next;
            }

            @Override
            public int previousIndex() {
                return next - 1;
            }

            @Override
            public void remove() {
                throw new UnsupportedOperationException("Can't change the size of an inventory!");
            }

            @Override
            public void set(ItemStack item) {
                if (last == -1) {
                    throw new IllegalStateException("No current item!");
                }
                setItem(last, item);
            }

            @Override
            public void add(ItemStack item) {
                throw new UnsupportedOperationException("Can't change the size of an inventory!");
            }
        };
    }

    /** A block's inventory is where the block is; the others are nowhere. */
    @Override
    public Location getLocation() {
        if (kind != 4) {
            return null;
        }
        FerraloomWorld world = Bridge.server.world((int) a);
        int x = (int) (b >> 38);
        int y = (int) (b << 52 >> 52);
        int z = (int) (b << 26 >> 38);
        return world == null ? null : new Location(world, x, y, z);
    }

    @Override
    public String toString() {
        return "FerraloomInventory{" + type + "}";
    }
}
