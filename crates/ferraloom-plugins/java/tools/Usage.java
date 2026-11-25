import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import org.bukkit.plugin.PluginDescriptionFile;

/**
 * What real plugins need of the bridge: reads plugin jars' bytecode and
 * reports every {@code org.bukkit} method, field and event they reference,
 * marked by what a call would meet on Ferraloom, plus their uses of server
 * internals ({@code net.minecraft}, {@code org.bukkit.craftbukkit}).
 *
 * <p>A method's status comes from the classes that can answer it at run
 * time: the bridge's classes and the API's own that implement the type the
 * plugin calls it on. {@code implemented}: every bridge class overrides it;
 * {@code partial}: some do; {@code stubbed}: the bridge class inherits the
 * generated stub (or the API's "Not supported yet" body); {@code absent}: no
 * class implements the type at all; {@code missing}: the Spigot API has no
 * such method (Paper API, or removed). A call into the API's own code
 * ({@code ItemStack.getItemMeta}, {@code Bukkit.getOnlinePlayers}, a class's
 * static initializer) is followed, and what it reaches is counted as an
 * implied use. Events are {@code bridged} when their handler list is one of
 * {@code Bridge.EVENTS}.
 *
 * <pre>java -cp spigot-api.jar:tools Usage API_JAR BRIDGE_JAR PLUGIN_JAR_OR_FOLDER... &gt; usage.md</pre>
 */
public final class Usage {
    enum Status { IMPLEMENTED, PARTIAL, STUBBED, ABSENT, MISSING, API }

    // Class files: just what the report needs.

    record Method(String name, String desc, int access, byte[] code) {
        boolean isAbstract() {
            return (access & 0x0400) != 0;
        }

        boolean isStatic() {
            return (access & 0x0008) != 0;
        }
    }

    record Field(String name, String desc) {
    }

    static final class Klass {
        String name;
        String superName;
        int access;
        List<String> interfaces = new ArrayList<>();
        Map<String, Method> methods = new LinkedHashMap<>();
        Set<String> fields = new HashSet<>();
        /** The constant pool: tag and raw values. */
        int[] tags;
        Object[] values;
        boolean isInterface() {
            return (access & 0x0200) != 0;
        }

        boolean isAbstract() {
            return (access & 0x0400) != 0;
        }

        String utf(int i) {
            return (String) values[i];
        }

        String className(int i) {
            return utf((Integer) values[i]);
        }

        /** A Fieldref/Methodref/InterfaceMethodref: owner, name, descriptor. */
        String[] ref(int i) {
            int[] r = (int[]) values[i];
            int[] nat = (int[]) values[r[1]];
            return new String[] {className(r[0]), utf(nat[0]), utf(nat[1])};
        }
    }

    static Klass parse(byte[] bytes) throws IOException {
        DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes));
        if (in.readInt() != 0xCAFEBABE) {
            throw new IOException("not a class file");
        }
        in.readUnsignedShort();
        in.readUnsignedShort();
        Klass k = new Klass();
        int count = in.readUnsignedShort();
        k.tags = new int[count];
        k.values = new Object[count];
        for (int i = 1; i < count; i++) {
            int tag = in.readUnsignedByte();
            k.tags[i] = tag;
            switch (tag) {
                case 1 -> k.values[i] = in.readUTF();
                case 3, 4 -> in.readInt();
                case 5, 6 -> {
                    in.readLong();
                    i++;
                }
                case 7, 8, 16, 19, 20 -> k.values[i] = in.readUnsignedShort();
                case 9, 10, 11, 12, 17, 18 -> k.values[i] = new int[] {in.readUnsignedShort(), in.readUnsignedShort()};
                case 15 -> {
                    in.readUnsignedByte();
                    in.readUnsignedShort();
                }
                default -> throw new IOException("constant pool tag " + tag);
            }
        }
        k.access = in.readUnsignedShort();
        k.name = k.className(in.readUnsignedShort());
        int sup = in.readUnsignedShort();
        k.superName = sup == 0 ? null : k.className(sup);
        for (int n = in.readUnsignedShort(); n > 0; n--) {
            k.interfaces.add(k.className(in.readUnsignedShort()));
        }
        for (int n = in.readUnsignedShort(); n > 0; n--) {
            in.readUnsignedShort();
            String name = k.utf(in.readUnsignedShort());
            String desc = k.utf(in.readUnsignedShort());
            k.fields.add(name + ":" + desc);
            skipAttributes(in);
        }
        for (int n = in.readUnsignedShort(); n > 0; n--) {
            int access = in.readUnsignedShort();
            String name = k.utf(in.readUnsignedShort());
            String desc = k.utf(in.readUnsignedShort());
            byte[] code = null;
            for (int a = in.readUnsignedShort(); a > 0; a--) {
                String attr = k.utf(in.readUnsignedShort());
                byte[] body = new byte[in.readInt()];
                in.readFully(body);
                if (attr.equals("Code")) {
                    int length = ((body[4] & 0xff) << 24) | ((body[5] & 0xff) << 16) | ((body[6] & 0xff) << 8) | (body[7] & 0xff);
                    code = java.util.Arrays.copyOfRange(body, 8, 8 + length);
                }
            }
            k.methods.put(name + desc, new Method(name, desc, access, code));
        }
        return k;
    }

    static void skipAttributes(DataInputStream in) throws IOException {
        for (int a = in.readUnsignedShort(); a > 0; a--) {
            in.readUnsignedShort();
            in.skipNBytes(in.readInt());
        }
    }

    /** Each instruction's constant pool operand worth following: {opcode, index}. */
    static List<int[]> instructions(byte[] code) {
        List<int[]> out = new ArrayList<>();
        int pc = 0;
        while (pc < code.length) {
            int op = code[pc] & 0xff;
            int index = pc + 2 < code.length ? ((code[pc + 1] & 0xff) << 8) | (code[pc + 2] & 0xff) : 0;
            switch (op) {
                case 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xbb -> out.add(new int[] {op, index});
                default -> {
                }
            }
            pc += length(code, pc, op);
        }
        return out;
    }

    static int length(byte[] code, int pc, int op) {
        if (op == 0xaa || op == 0xab) {
            int p = (pc + 4) & ~3;
            if (op == 0xaa) {
                int low = readInt(code, p + 4);
                int high = readInt(code, p + 8);
                return p + 12 + 4 * (high - low + 1) - pc;
            }
            return p + 8 + 8 * readInt(code, p + 4) - pc;
        }
        if (op == 0xc4) {
            return (code[pc + 1] & 0xff) == 0x84 ? 6 : 4;
        }
        return switch (op) {
            case 0x10, 0x12, 0x15, 0x16, 0x17, 0x18, 0x19, 0x36, 0x37, 0x38, 0x39, 0x3a, 0xa9, 0xbc -> 2;
            case 0x11, 0x13, 0x14, 0x84, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xbb, 0xbd, 0xc0, 0xc1, 0xc6, 0xc7 -> 3;
            case 0xc5 -> 4;
            case 0xb9, 0xba, 0xc8, 0xc9 -> 5;
            default -> op >= 0x99 && op <= 0xa8 ? 3 : 1;
        };
    }

    static int readInt(byte[] b, int p) {
        return ((b[p] & 0xff) << 24) | ((b[p + 1] & 0xff) << 16) | ((b[p + 2] & 0xff) << 8) | (b[p + 3] & 0xff);
    }

    // The model: the API's and the bridge's classes, and one plugin's over them.

    static final Map<String, Klass> BASE = new HashMap<>();
    static final Set<String> BRIDGE = new HashSet<>();
    static final String STUBS = "dev/ferraloom/bukkit/stub/";
    /** Event classes whose handler list Bridge.EVENTS holds. */
    static final Set<String> BRIDGED = new HashSet<>();
    static final Map<String, Set<String>> ANCESTORS = new HashMap<>();
    static final Map<String, List<String>> IMPLEMENTORS = new HashMap<>();

    Map<String, Klass> plugin = new HashMap<>();

    Klass klass(String name) {
        Klass k = plugin.get(name);
        return k != null ? k : BASE.get(name);
    }

    static Set<String> ancestors(String name) {
        Set<String> cached = ANCESTORS.get(name);
        if (cached != null) {
            return cached;
        }
        Set<String> out = new LinkedHashSet<>();
        Klass k = BASE.get(name);
        if (k != null) {
            for (String parent : parents(k)) {
                out.add(parent);
                out.addAll(ancestors(parent));
            }
        }
        ANCESTORS.put(name, out);
        return out;
    }

    static List<String> parents(Klass k) {
        List<String> out = new ArrayList<>(k.interfaces);
        if (k.superName != null) {
            out.add(0, k.superName);
        }
        return out;
    }

    /** The concrete API and bridge classes a value of {@code type} can be (stubs aside). */
    static List<String> implementors(String type) {
        return IMPLEMENTORS.computeIfAbsent(type, t -> BASE.values().stream()
                .filter(k -> !k.isInterface() && !k.isAbstract() && !k.name.startsWith(STUBS))
                .filter(k -> k.name.equals(t) || ancestors(k.name).contains(t))
                .map(k -> k.name).sorted().toList());
    }

    /** JVM method selection: the class chain's concrete method, else a superinterface's default. Null when abstract only. */
    record Found(Klass owner, Method method) {
    }

    Found select(String type, String key) {
        for (String c = type; c != null; ) {
            Klass k = klass(c);
            if (k == null) {
                break;
            }
            Method m = k.methods.get(key);
            if (m != null && !m.isAbstract()) {
                return new Found(k, m);
            }
            c = k.superName;
        }
        return defaultMethod(type, key, new HashSet<>());
    }

    Found defaultMethod(String type, String key, Set<String> seen) {
        Klass k = klass(type);
        if (k == null || !seen.add(type)) {
            return null;
        }
        for (String parent : parents(k)) {
            Klass p = klass(parent);
            if (p != null && p.isInterface()) {
                Method m = p.methods.get(key);
                if (m != null && !m.isAbstract() && !m.isStatic()) {
                    return new Found(p, m);
                }
            }
            Found f = defaultMethod(parent, key, seen);
            if (f != null) {
                return f;
            }
        }
        return null;
    }

    /** JVM method resolution: the class that declares {@code key} for {@code type}, or null. */
    Klass declaring(String type, String key, Set<String> seen) {
        Klass k = klass(type);
        if (k == null || !seen.add(type)) {
            return null;
        }
        if (k.methods.containsKey(key)) {
            return k;
        }
        for (String parent : parents(k)) {
            Klass d = declaring(parent, key, seen);
            if (d != null) {
                return d;
            }
        }
        return null;
    }

    boolean hasField(String type, String key, Set<String> seen) {
        Klass k = klass(type);
        if (k == null || !seen.add(type)) {
            return false;
        }
        if (k.fields.contains(key)) {
            return true;
        }
        for (String parent : parents(k)) {
            if (hasField(parent, key, seen)) {
                return true;
            }
        }
        return false;
    }

    /** "Not supported yet": a body that only throws UnsupportedOperationException. */
    static boolean unsupported(Klass k, Method m) {
        if (m.code() == null || m.code().length > 16 || (m.code()[0] & 0xff) != 0xbb) {
            return false;
        }
        int index = ((m.code()[1] & 0xff) << 8) | (m.code()[2] & 0xff);
        return k.className(index).equals("java/lang/UnsupportedOperationException");
    }

    static boolean isBukkit(String name) {
        return name.startsWith("org/bukkit/") && !name.startsWith("org/bukkit/craftbukkit/");
    }

    // One plugin's references.

    /** A use: what is used, its status, and whether the plugin names it itself. */
    record Use(String key, String area, Status status, boolean direct) {
    }

    final Map<String, Use> uses = new TreeMap<>();
    final Set<String> walked = new HashSet<>();
    final Deque<Runnable> work = new ArrayDeque<>();

    void use(String key, String area, Status status, boolean direct) {
        Use old = uses.get(key);
        if (old == null || (direct && !old.direct())) {
            uses.put(key, new Use(key, area, status, direct));
        }
    }

    /** A call to {@code owner.name desc}, as the plugin (direct) or API code it reaches makes it. */
    void call(String owner, String name, String desc, boolean direct) {
        if (owner.startsWith("[")) {
            return;
        }
        String key = name + desc;
        Klass ownerClass = klass(owner);
        if (ownerClass == null) {
            if (isBukkit(owner)) {
                use(label(owner, name, desc), area(owner, name), Status.MISSING, direct);
            }
            return;
        }
        Klass decl = declaring(owner, key, new HashSet<>());
        if (decl == null) {
            // Declared outside the model (java.lang.Enum, a library's class): not the API's to answer.
            if (!knownHierarchy(owner)) {
                return;
            }
            if (isBukkit(owner) || ancestorsAll(owner).stream().anyMatch(Usage::isBukkit)) {
                String api = isBukkit(owner) ? owner : ancestorsAll(owner).stream().filter(Usage::isBukkit).findFirst().get();
                if (!name.equals("<init>") && !isObjectMethod(key)) {
                    use(label(api, name, desc), area(api, name), Status.MISSING, direct);
                }
            }
            return;
        }
        if (!isBukkit(decl.name)) {
            return;
        }
        Method declared = decl.methods.get(key);
        // Static methods, constructors and calls on the plugin's own classes run one known body.
        if (declared.isStatic() || name.equals("<init>") || plugin.containsKey(owner)) {
            Found f = declared.isStatic() || name.equals("<init>") ? new Found(decl, declared) : select(owner, key);
            if (f != null && isBukkit(f.owner().name)) {
                String apiOwner = plugin.containsKey(owner) ? f.owner().name : owner;
                if (unsupported(f.owner(), f.method())) {
                    use(label(apiOwner, name, desc), area(apiOwner, name), Status.STUBBED, direct);
                } else {
                    follow(f);
                }
            }
            if (!name.equals("<init>") || isBukkit(owner)) {
                touch(decl.name);
            }
            return;
        }
        if ((ownerClass.access & 0x2000) != 0) {
            return; // An annotation's methods: the JVM implements them.
        }
        List<String> bridge = new ArrayList<>();
        List<String> api = new ArrayList<>();
        for (String impl : implementors(owner)) {
            (BRIDGE.contains(impl) ? bridge : api).add(impl);
        }
        for (Klass k : plugin.values()) {
            if (!k.isInterface() && !k.isAbstract() && ancestorsAll(k.name).contains(owner)) {
                api.add(k.name);
            }
        }
        Status status;
        if (!bridge.isEmpty()) {
            int done = 0;
            for (String impl : bridge) {
                Found f = select(impl, key);
                if (f != null && !f.owner().name.startsWith(STUBS) && !unsupported(f.owner(), f.method())) {
                    done++;
                    if (isBukkit(f.owner().name)) {
                        follow(f);
                    }
                }
            }
            status = done == bridge.size() ? Status.IMPLEMENTED : done == 0 ? Status.STUBBED : Status.PARTIAL;
        } else if (!api.isEmpty()) {
            int done = 0;
            for (String impl : api) {
                Found f = select(impl, key);
                if (f != null && !unsupported(f.owner(), f.method())) {
                    done++;
                    if (isBukkit(f.owner().name)) {
                        follow(f);
                    }
                }
            }
            status = done > 0 ? Status.API : Status.STUBBED;
        } else {
            Found f = select(owner, key);
            if (f != null && !unsupported(f.owner(), f.method())) {
                follow(f);
                status = Status.API;
            } else {
                status = Status.ABSENT;
            }
        }
        if (status != Status.API) {
            use(label(owner, name, desc), area(owner, name), status, direct);
        }
    }

    /** Every ancestor is in the model, so a method none declares really is not there. */
    boolean knownHierarchy(String name) {
        return ancestorsAll(name).stream().allMatch(c -> c.equals("java/lang/Object") || klass(c) != null);
    }

    Set<String> ancestorsAll(String name) {
        Set<String> out = new LinkedHashSet<>();
        Deque<String> todo = new ArrayDeque<>(List.of(name));
        while (!todo.isEmpty()) {
            Klass k = klass(todo.pop());
            if (k != null) {
                for (String p : parents(k)) {
                    if (out.add(p)) {
                        todo.push(p);
                    }
                }
            }
        }
        return out;
    }

    static boolean isObjectMethod(String key) {
        return Set.of("hashCode()I", "equals(Ljava/lang/Object;)Z", "toString()Ljava/lang/String;", "getClass()Ljava/lang/Class;")
                .contains(key);
    }

    void field(String owner, String name, String desc, boolean direct) {
        if (!isBukkit(owner)) {
            return;
        }
        if (klass(owner) == null || !hasField(owner, name + ":" + desc, new HashSet<>())) {
            use(simple(owner) + "." + name, area(owner, name), Status.MISSING, direct);
            return;
        }
        touch(owner);
    }

    /** A class initialized: its static initializer runs. */
    void touch(String owner) {
        Klass k = klass(owner);
        if (k == null || !isBukkit(owner)) {
            return;
        }
        Method clinit = k.methods.get("<clinit>()V");
        if (clinit != null) {
            follow(new Found(k, clinit));
        }
        if (k.superName != null && isBukkit(k.superName)) {
            touch(k.superName);
        }
    }

    /** API code the plugin reaches: what it calls is an implied use. */
    void follow(Found f) {
        if (f.method().code() == null || !walked.add(f.owner().name + "." + f.method().name() + f.method().desc())) {
            return;
        }
        Klass k = f.owner();
        work.add(() -> {
            for (int[] insn : instructions(f.method().code())) {
                switch (insn[0]) {
                    case 0xb2, 0xb3 -> {
                        String[] r = k.ref(insn[1]);
                        field(r[0], r[1], r[2], false);
                    }
                    case 0xb6, 0xb7, 0xb8, 0xb9 -> {
                        String[] r = k.ref(insn[1]);
                        call(r[0], r[1], r[2], false);
                    }
                    case 0xbb -> touch(k.className(insn[1]));
                    default -> {
                    }
                }
            }
        });
    }

    void drain() {
        while (!work.isEmpty()) {
            work.pop().run();
        }
    }

    // Labels and areas.

    static String simple(String internal) {
        return internal.substring(internal.lastIndexOf('/') + 1).replace('$', '.');
    }

    static String label(String owner, String name, String desc) {
        StringBuilder s = new StringBuilder(simple(owner)).append('.').append(name).append('(');
        int i = 1;
        boolean first = true;
        while (desc.charAt(i) != ')') {
            int start = i;
            while (desc.charAt(i) == '[') {
                i++;
            }
            String type;
            if (desc.charAt(i) == 'L') {
                int end = desc.indexOf(';', i);
                type = simple(desc.substring(i + 1, end));
                i = end + 1;
            } else {
                type = switch (desc.charAt(i)) {
                    case 'Z' -> "boolean";
                    case 'B' -> "byte";
                    case 'C' -> "char";
                    case 'S' -> "short";
                    case 'I' -> "int";
                    case 'J' -> "long";
                    case 'F' -> "float";
                    default -> "double";
                };
                i++;
            }
            int dims = 0;
            while (desc.charAt(start + dims) == '[') {
                dims++;
            }
            s.append(first ? "" : ", ").append(type).append("[]".repeat(dims));
            first = false;
        }
        return s.append(')').toString();
    }

    static final Pattern INVENTORY_ON_PLAYER = Pattern.compile(
            "(?i).*(inventory|itemin|itemoncursor|openworkbench|openenchanting|openmerchant|openanvil|enderchest|openbook|cooldown|openloom|opencartography|opengrindstone|opensmithing|openstonecutter).*");

    /** The fan-out area owning a type (docs/ferraloom/plugins.md, "Fan-out"). */
    static String area(String owner, String name) {
        String pkg = owner.substring(0, owner.lastIndexOf('/') + 1);
        String cls = owner.substring(pkg.length());
        String top = cls.contains("$") ? cls.substring(0, cls.indexOf('$')) : cls;
        if (pkg.equals("org/bukkit/entity/") && (top.equals("Player") || top.equals("HumanEntity"))) {
            return INVENTORY_ON_PLAYER.matcher(name).matches() ? "inventory" : "players";
        }
        if (owner.equals("org/bukkit/Server") || owner.equals("org/bukkit/Bukkit")) {
            if (name.matches("getItemFactory|createInventory|createMerchant|createExplorerMap|createMap|getMap")) {
                return "inventory";
            }
            if (name.matches("createBlockData|getTag|getTags|getLootTable")) {
                return "blocks";
            }
            if (name.matches("getEntity|selectEntities|getEntityFactory")) {
                return "entities";
            }
            if (name.matches("(get|create|unload)World.*|getWorlds|createChunkData|getViewDistance|getSimulationDistance|getSpawnRadius|"
                    + "getAllowEnd|getAllowNether|getGenerateStructures|getWorldType|getLevelName|createWorldBorder|getDefaultGameMode")) {
                return "worlds";
            }
            return "server";
        }
        if (pkg.equals("org/bukkit/") && top.equals("OfflinePlayer") || pkg.startsWith("org/bukkit/event/player/")) {
            return "players";
        }
        if (pkg.startsWith("org/bukkit/block/") || pkg.startsWith("org/bukkit/event/block/")
                || pkg.equals("org/bukkit/") && Set.of("Material", "Tag", "Axis", "Instrument", "Note").contains(top)) {
            return "blocks";
        }
        if (pkg.startsWith("org/bukkit/event/world/") || pkg.startsWith("org/bukkit/event/weather/") || pkg.startsWith("org/bukkit/generator/")
                || pkg.equals("org/bukkit/") && Set.of("World", "Chunk", "ChunkSnapshot", "WorldBorder", "WorldCreator", "GameRule",
                        "HeightMap", "TreeType", "Difficulty", "RegionAccessor", "Location", "Raid", "StructureType").contains(top)) {
            return "worlds";
        }
        if (pkg.startsWith("org/bukkit/entity/") || pkg.startsWith("org/bukkit/event/entity/") || pkg.startsWith("org/bukkit/event/vehicle/")
                || pkg.startsWith("org/bukkit/event/hanging/") || pkg.startsWith("org/bukkit/event/raid/") || pkg.startsWith("org/bukkit/potion/")
                || pkg.startsWith("org/bukkit/attribute/") || pkg.startsWith("org/bukkit/damage/") || pkg.startsWith("org/bukkit/projectiles/")
                || pkg.equals("org/bukkit/") && Set.of("EntityEffect", "Art").contains(top)) {
            return "entities";
        }
        if (pkg.startsWith("org/bukkit/inventory/") || pkg.startsWith("org/bukkit/event/inventory/") || pkg.startsWith("org/bukkit/enchantments/")
                || pkg.startsWith("org/bukkit/event/enchantment/") || pkg.startsWith("org/bukkit/persistence/") || pkg.startsWith("org/bukkit/map/")
                || pkg.startsWith("org/bukkit/event/player/PlayerItem")) {
            return "inventory";
        }
        return "server";
    }

    static final List<String> AREAS = List.of("players", "blocks", "worlds", "entities", "inventory", "server");

    // Server internals.

    static final Pattern INTERNAL = Pattern.compile("(net/minecraft/|org/bukkit/craftbukkit/)[\\w/$]*");
    static final Pattern INTERNAL_STRING = Pattern.compile("(net\\.minecraft\\.|org\\.bukkit\\.craftbukkit)[\\w.$]*");
    static final Pattern VERSIONED = Pattern.compile("(?i)(^|[/$_.])(v?1_\\d+(_R\\d+)?|v\\d+_\\d+\\w*|nms|r\\d+|mc\\d+|paper|folia|spigot|legacy|modern|v\\d{3,})($|[/$_.])");

    record Internal(String cls, String first, boolean reachable, boolean versioned, boolean reflective) {
    }

    // The report.

    record Report(String file, String name, String version, String apiVersion, List<String> libraries, Map<String, Use> uses,
            Set<String> events, List<Internal> internals, int paperRefs) {
    }

    static String eventKey(String internal) {
        return simple(internal);
    }

    /** Reads every class of a jar and of the jars inside it. */
    static void readJar(InputStream stream, Map<String, Klass> into, Set<String> nested, Map<String, byte[]> files) throws IOException {
        ZipInputStream zip = new ZipInputStream(stream);
        for (ZipEntry e; (e = zip.getNextEntry()) != null; ) {
            String n = e.getName();
            if (n.endsWith(".class") && !n.startsWith("META-INF/versions/")) {
                try {
                    Klass k = parse(zip.readAllBytes());
                    into.putIfAbsent(k.name, k);
                    if (nested != null) {
                        nested.add(k.name);
                    }
                } catch (IOException | RuntimeException ex) {
                    // Not a class we can read: skipped.
                }
            } else if (n.endsWith(".jar") || n.endsWith(".jarinjar")) {
                readJar(new ByteArrayInputStream(zip.readAllBytes()), into, nested == null ? new HashSet<>() : nested, null);
            } else if (files != null && (n.equals("plugin.yml") || n.equals("paper-plugin.yml"))) {
                files.put(n, zip.readAllBytes());
            }
        }
    }

    static Report analyze(Path jar) throws IOException {
        Usage u = new Usage();
        Set<String> nested = new HashSet<>();
        Map<String, byte[]> files = new HashMap<>();
        Map<String, Klass> outer = new HashMap<>();
        try (InputStream in = Files.newInputStream(jar)) {
            ZipInputStream zip = new ZipInputStream(in);
            for (ZipEntry e; (e = zip.getNextEntry()) != null; ) {
                String n = e.getName();
                if (n.endsWith(".class") && !n.startsWith("META-INF/versions/")) {
                    try {
                        Klass k = parse(zip.readAllBytes());
                        outer.putIfAbsent(k.name, k);
                    } catch (IOException | RuntimeException ex) {
                        // Skipped.
                    }
                } else if (n.endsWith(".jar") || n.endsWith(".jarinjar")) {
                    readJar(new ByteArrayInputStream(zip.readAllBytes()), u.plugin, nested, files);
                } else if (n.equals("plugin.yml") || n.equals("paper-plugin.yml")) {
                    files.putIfAbsent(n, zip.readAllBytes());
                }
            }
        }
        u.plugin.putAll(outer);
        String name = jar.getFileName().toString();
        String version = "?";
        String apiVersion = "";
        String main = null;
        List<String> libraries = List.of();
        byte[] yml = files.getOrDefault("plugin.yml", files.get("paper-plugin.yml"));
        if (yml != null) {
            try {
                PluginDescriptionFile d = new PluginDescriptionFile(new ByteArrayInputStream(yml));
                name = d.getName();
                version = d.getVersion();
                apiVersion = d.getAPIVersion() == null ? "" : d.getAPIVersion();
                main = d.getMain().replace('.', '/');
                libraries = d.getLibraries();
            } catch (Exception ex) {
                // A paper-plugin.yml Bukkit cannot read: the jar's name stands in.
            }
        }

        // Direct references, then the API code they reach.
        Set<String> events = new TreeSet<>();
        Pattern bukkitType = Pattern.compile("L(org/bukkit/[\\w/$]+);");
        int paper = 0;
        for (Klass k : u.plugin.values()) {
            for (int i = 1; i < k.tags.length; i++) {
                switch (k.tags[i]) {
                    case 9 -> {
                        String[] r = k.ref(i);
                        u.field(r[0], r[1], r[2], true);
                    }
                    case 10, 11 -> {
                        String[] r = k.ref(i);
                        u.call(r[0], r[1], r[2], true);
                    }
                    case 7 -> {
                        String c = k.className(i);
                        if (isBukkit(c)) {
                            u.eventOrClass(c, events);
                        }
                        if (c.startsWith("com/destroystokyo/paper/") || c.startsWith("io/papermc/paper/")) {
                            paper++;
                        }
                    }
                    case 1 -> {
                        Matcher m = bukkitType.matcher(k.utf(i));
                        while (m.find()) {
                            u.eventOrClass(m.group(1), events);
                        }
                    }
                    default -> {
                    }
                }
            }
        }
        u.drain();

        // Server internals: which classes name them, and whether the plugin's main class reaches those.
        Set<String> reachable = new HashSet<>(nested);
        if (main != null) {
            Deque<String> todo = new ArrayDeque<>(List.of(main));
            reachable.add(main);
            while (!todo.isEmpty()) {
                Klass k = u.plugin.get(todo.pop());
                if (k == null) {
                    continue;
                }
                for (String c : u.referencedClasses(k)) {
                    if (u.plugin.containsKey(c) && reachable.add(c)) {
                        todo.push(c);
                    }
                }
            }
        }
        List<Internal> internals = new ArrayList<>();
        for (Klass k : u.plugin.values()) {
            String first = null;
            boolean reflective = false;
            for (int i = 1; i < k.tags.length && first == null; i++) {
                if (k.tags[i] == 1) {
                    Matcher m = INTERNAL.matcher(k.utf(i));
                    if (m.find()) {
                        first = m.group();
                    }
                }
            }
            if (first == null) {
                for (int i = 1; i < k.tags.length && first == null; i++) {
                    if (k.tags[i] == 8) {
                        Matcher m = INTERNAL_STRING.matcher(k.utf((Integer) k.values[i]));
                        if (m.find()) {
                            first = m.group();
                            reflective = true;
                        }
                    }
                }
            }
            if (first != null) {
                internals.add(new Internal(k.name, first.replace('/', '.'), reachable.contains(k.name),
                        VERSIONED.matcher(k.name).find(), reflective));
            }
        }
        internals.sort(Comparator.comparing(Internal::cls));
        return new Report(jar.getFileName().toString(), name, version, apiVersion, libraries, u.uses, events, internals, paper);
    }

    void eventOrClass(String c, Set<String> events) {
        Klass k = klass(c);
        if (k == null) {
            use(simple(c), area(c, ""), Status.MISSING, true);
            return;
        }
        if (ancestors(c).contains("org/bukkit/event/Event")) {
            events.add(c);
        }
    }

    Set<String> referencedClasses(Klass k) {
        Set<String> out = new HashSet<>();
        for (int i = 1; i < k.tags.length; i++) {
            if (k.tags[i] == 7) {
                out.add(k.className(i));
            } else if (k.tags[i] == 1) {
                Matcher m = Pattern.compile("L([\\w/$]+);").matcher(k.utf(i));
                while (m.find()) {
                    out.add(m.group(1));
                }
            }
        }
        return out;
    }

    /** The event's handler list's class: the nearest class with a static getHandlerList. */
    static String handlerOwner(String event) {
        for (String c = event; c != null; ) {
            Klass k = BASE.get(c);
            if (k == null) {
                return null;
            }
            Method m = k.methods.get("getHandlerList()Lorg/bukkit/event/HandlerList;");
            if (m != null && m.isStatic()) {
                return c;
            }
            c = k.superName;
        }
        return null;
    }

    public static void main(String[] args) throws Exception {
        for (String jar : new String[] {args[0], args[1]}) {
            Set<String> seen = new HashSet<>();
            try (InputStream in = Files.newInputStream(Path.of(jar))) {
                ZipInputStream zip = new ZipInputStream(in);
                for (ZipEntry e; (e = zip.getNextEntry()) != null; ) {
                    if (e.getName().endsWith(".class") && (e.getName().startsWith("org/bukkit/") || e.getName().startsWith("dev/ferraloom/"))) {
                        Klass k = parse(zip.readAllBytes());
                        BASE.put(k.name, k);
                        seen.add(k.name);
                    }
                }
            }
            if (jar.equals(args[1])) {
                BRIDGE.addAll(seen);
            }
        }
        // Fired: an event the API's or the bridge's code constructs (SimplePluginManager fires PluginEnableEvent,
        // the bridge's *Events classes theirs), or one whose handler list Bridge.EVENTS watches.
        for (Klass k : BASE.values()) {
            if (k.name.startsWith(STUBS) || ancestors(k.name).contains("org/bukkit/event/Event")) {
                continue;
            }
            for (Method m : k.methods.values()) {
                if (m.code() == null) {
                    continue;
                }
                for (int[] insn : instructions(m.code())) {
                    if (insn[0] == 0xbb && ancestors(k.className(insn[1])).contains("org/bukkit/event/Event")) {
                        String owner = handlerOwner(k.className(insn[1]));
                        if (owner != null) {
                            BRIDGED.add(owner);
                        }
                    }
                }
            }
        }
        Klass bridge = BASE.get("dev/ferraloom/bukkit/Bridge");
        for (int[] insn : instructions(bridge.methods.get("<clinit>()V").code())) {
            if (insn[0] == 0xb8) {
                String[] r = bridge.ref(insn[1]);
                if (r[1].equals("getHandlerList")) {
                    BRIDGED.add(r[0]);
                }
            }
        }
        List<Path> jars = new ArrayList<>();
        for (int i = 2; i < args.length; i++) {
            Path p = Path.of(args[i]);
            if (Files.isDirectory(p)) {
                try (var walk = Files.walk(p)) {
                    walk.filter(f -> f.toString().endsWith(".jar")).sorted().forEach(jars::add);
                }
            } else {
                jars.add(p);
            }
        }
        List<Report> reports = new ArrayList<>();
        for (Path jar : jars) {
            reports.add(analyze(jar));
        }
        reports.sort(Comparator.comparing(r -> r.name().toLowerCase()));
        print(reports);
    }

    /** What a call meets on Ferraloom that does not work: stubs, partial overrides, no implementation. */
    static boolean missed(Status s) {
        return s == Status.STUBBED || s == Status.PARTIAL || s == Status.ABSENT;
    }

    static String unbridged(String event) {
        String owner = handlerOwner(event);
        if (owner == null) {
            return null;
        }
        return BRIDGED.contains(owner) ? null : owner;
    }

    /** The methods of {@code area} (or every area when null) whose status passes {@code which}, ranked by how many plugins need them. */
    static void ranking(StringBuilder s, List<Report> reports, String area, java.util.function.Predicate<Status> which, int limit) {
        Map<String, List<Report>> byKey = new TreeMap<>();
        Map<String, Status> status = new HashMap<>();
        Map<String, Integer> directCount = new HashMap<>();
        for (Report r : reports) {
            for (Use x : r.uses().values()) {
                if ((area == null || x.area().equals(area)) && which.test(x.status())) {
                    byKey.computeIfAbsent(x.key(), k -> new ArrayList<>()).add(r);
                    status.put(x.key(), x.status());
                    if (x.direct()) {
                        directCount.merge(x.key(), 1, Integer::sum);
                    }
                }
            }
        }
        s.append("| Method | Status | Plugins | Direct | Which |\n|---|---|---:|---:|---|\n");
        byKey.entrySet().stream()
                .sorted(Comparator.<Map.Entry<String, List<Report>>>comparingInt(e -> -directCount.getOrDefault(e.getKey(), 0))
                        .thenComparingInt(e -> -e.getValue().size()).thenComparing(Map.Entry::getKey))
                .limit(limit)
                .forEach(e -> s.append(String.format("| `%s` | %s | %d | %d | %s |%n", e.getKey(),
                        status.get(e.getKey()).name().toLowerCase(), e.getValue().size(), directCount.getOrDefault(e.getKey(), 0),
                        e.getValue().stream().map(Report::name).collect(Collectors.joining(", ")))));
        if (byKey.size() > limit) {
            s.append(String.format("%n%d more, each needed by fewer plugins.%n", byKey.size() - limit));
        }
    }

    static void print(List<Report> reports) {
        StringBuilder s = new StringBuilder();
        s.append("## Per plugin\n\n");
        s.append("Misses: distinct stubbed, partial or absent methods it names itself, plus unbridged events it references; "
                + "implied: more reached through the API's own code (`Bukkit.*`, `ItemStack`, static initializers).\n\n");
        s.append("| Plugin | Version | api-version | Bukkit methods named | Implemented | Misses | Implied misses | Not in Spigot API | Unbridged events | Internals (main / adapter / reflective) | Paper classes | libraries |\n");
        s.append("|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|---|\n");
        for (Report r : reports) {
            long direct = r.uses().values().stream().filter(Use::direct).count();
            long implemented = r.uses().values().stream().filter(x -> x.direct() && x.status() == Status.IMPLEMENTED).count();
            long misses = r.uses().values().stream().filter(x -> x.direct() && missed(x.status())).count();
            long implied = r.uses().values().stream().filter(x -> !x.direct() && missed(x.status())).count();
            long missing = r.uses().values().stream().filter(x -> x.direct() && x.status() == Status.MISSING).count();
            long events = r.events().stream().filter(e -> unbridged(e) != null).count();
            long mainPath = r.internals().stream().filter(i -> i.reachable() && !i.versioned() && !i.reflective()).count();
            long reflective = r.internals().stream().filter(Internal::reflective).count();
            long adapter = r.internals().size() - mainPath - reflective;
            s.append(String.format("| %s | %s | %s | %d | %d | %d | %d | %d | %d | %d / %d / %d | %d | %s |%n", r.name(), r.version(),
                    r.apiVersion(), direct, implemented, misses + events, implied, missing, events, mainPath, adapter, reflective,
                    r.paperRefs(), r.libraries().isEmpty() ? "" : String.join(", ", r.libraries())));
        }

        for (String area : AREAS) {
            s.append("\n## ").append(Character.toUpperCase(area.charAt(0))).append(area.substring(1)).append("\n\n");
            Map<String, List<Report>> eventUsers = new TreeMap<>();
            for (Report r : reports) {
                for (String e : r.events()) {
                    String owner = unbridged(e);
                    if (owner != null && area(owner, "").equals(area)) {
                        eventUsers.computeIfAbsent(simple(owner), k -> new ArrayList<>()).add(r);
                    }
                }
            }
            s.append("Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):\n\n");
            ranking(s, reports, area, Usage::missed, 60);
            s.append("\nUnbridged events, by plugins referencing them:\n\n");
            s.append("| Event | Plugins | Which |\n|---|---:|---|\n");
            eventUsers.entrySet().stream()
                    .sorted(Comparator.<Map.Entry<String, List<Report>>>comparingInt(e -> -e.getValue().size()).thenComparing(Map.Entry::getKey))
                    .forEach(e -> s.append(String.format("| `%s` | %d | %s |%n", e.getKey(), e.getValue().size(),
                            e.getValue().stream().map(Report::name).collect(Collectors.joining(", ")))));
        }

        s.append("\n## Not in the Spigot API\n\n");
        s.append("Paper or Folia API (plugins mostly call it after detecting Paper), or API removed since the plugin was built: "
                + "a NoSuchMethodError if reached on Ferraloom.\n\n");
        ranking(s, reports, null, st -> st == Status.MISSING, 30);

        s.append("\n## Server internals\n\n");
        s.append("Classes naming `net.minecraft` or `org.bukkit.craftbukkit`. Main: reached from the plugin's main class through "
                + "direct references, not version-named. Adapter: version-named or not reached directly (picked by version string). "
                + "Reflective: a string constant only.\n\n");
        for (Report r : reports) {
            if (r.internals().isEmpty()) {
                continue;
            }
            s.append("- **").append(r.name()).append("**: ");
            List<String> parts = new ArrayList<>();
            for (Internal i : r.internals()) {
                String kind = i.reflective() ? "reflective" : i.reachable() && !i.versioned() ? "main" : "adapter";
                parts.add("`" + simple(i.cls()) + "` (" + kind + ", " + i.first() + ")");
            }
            if (parts.size() > 8) {
                int more = parts.size() - 8;
                parts = new ArrayList<>(parts.subList(0, 8));
                parts.add(more + " more");
            }
            s.append(String.join(", ", parts)).append('\n');
        }
        System.out.print(s);
    }
}
