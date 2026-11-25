package dev.ferraloom.nmsplugin;

import org.bukkit.plugin.java.JavaPlugin;

/** Names a server internal the way compiled NMS code does: Ferraloom must refuse to load it. */
public final class NmsPlugin extends JavaPlugin {
    static final String SERVER = "net/minecraft/server/MinecraftServer";

    @Override
    public void onEnable() {
        getLogger().info("NmsPlugin enabled, which it must not be");
    }
}
