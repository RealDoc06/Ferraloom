import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.List;

/**
 * How much of each stubbed Bukkit interface Ferraloom implements: for each
 * stub, the methods it throws from and how many of them the bridge class
 * extending it overrides. Prints a Markdown table, then the total.
 *
 * <pre>java -cp spigot-api.jar:bridge-classes:tools Coverage dev.ferraloom.bukkit.FerraloomServer ...</pre>
 */
public final class Coverage {
    public static void main(String[] args) throws Exception {
        ClassLoader loader = Coverage.class.getClassLoader();
        int allMethods = 0;
        int allDone = 0;
        System.out.println("| Interface | Implemented | Methods | % |");
        System.out.println("|---|---:|---:|---:|");
        for (String name : args) {
            Class<?> impl = Class.forName(name, false, loader);
            Class<?> stub = impl;
            while (stub != null && !stub.getSimpleName().startsWith("Stub")) {
                stub = stub.getSuperclass();
            }
            if (stub == null) {
                throw new IllegalArgumentException(name + " extends no stub");
            }
            List<Method> methods = new ArrayList<>();
            for (Method m : stub.getDeclaredMethods()) {
                if (Modifier.isPublic(m.getModifiers()) && !m.isSynthetic() && !m.isBridge()) {
                    methods.add(m);
                }
            }
            int done = 0;
            for (Method m : methods) {
                for (Class<?> c = impl; c != stub; c = c.getSuperclass()) {
                    try {
                        c.getDeclaredMethod(m.getName(), m.getParameterTypes());
                        done++;
                        break;
                    } catch (NoSuchMethodException e) {
                        // Not in this class: its superclass may have it.
                    }
                }
            }
            String iface = stub.getInterfaces()[0].getCanonicalName();
            System.out.printf("| `%s` | %d | %d | %.1f |%n", iface, done, methods.size(), 100.0 * done / methods.size());
            allMethods += methods.size();
            allDone += done;
        }
        System.out.printf("| **Total** | %d | %d | %.1f |%n", allDone, allMethods, 100.0 * allDone / allMethods);
    }
}
