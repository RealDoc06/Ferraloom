package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubConsoleCommandSender;
import java.util.Set;
import java.util.UUID;
import net.md_5.bungee.api.chat.BaseComponent;
import net.md_5.bungee.api.chat.TextComponent;
import org.bukkit.Server;
import org.bukkit.permissions.PermissibleBase;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;
import org.bukkit.plugin.Plugin;

/** The server console: an operator with every permission, its replies in the server's log. */
final class FerraloomConsole extends StubConsoleCommandSender {
    private final FerraloomServer server;
    private PermissibleBase permissions;
    private final Spigot spigot = new Spigot() {
        @Override
        public void sendMessage(BaseComponent component) {
            FerraloomConsole.this.sendMessage(component.toLegacyText());
        }

        @Override
        public void sendMessage(BaseComponent... components) {
            FerraloomConsole.this.sendMessage(TextComponent.toLegacyText(components));
        }
    };

    FerraloomConsole(FerraloomServer server) {
        this.server = server;
    }

    /** PermissibleBase asks the plugin manager, which the server makes after the console. */
    private PermissibleBase permissions() {
        if (permissions == null) {
            permissions = new PermissibleBase(this);
        }
        return permissions;
    }

    @Override
    public String getName() {
        return "CONSOLE";
    }

    @Override
    public Server getServer() {
        return server;
    }

    @Override
    public void sendMessage(String message) {
        if (message != null) {
            ServerNative.consoleMessage(message);
        }
    }

    @Override
    public void sendMessage(String... messages) {
        for (String message : messages) {
            sendMessage(message);
        }
    }

    @Override
    public void sendMessage(UUID sender, String message) {
        sendMessage(message);
    }

    @Override
    public void sendMessage(UUID sender, String... messages) {
        sendMessage(messages);
    }

    @Override
    public void sendRawMessage(String message) {
        sendMessage(message);
    }

    @Override
    public void sendRawMessage(UUID sender, String message) {
        sendMessage(message);
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }

    @Override
    public boolean isOp() {
        return true;
    }

    @Override
    public boolean isPermissionSet(String name) {
        return permissions().isPermissionSet(name);
    }

    @Override
    public boolean isPermissionSet(Permission perm) {
        return permissions().isPermissionSet(perm);
    }

    @Override
    public boolean hasPermission(String name) {
        return permissions().hasPermission(name);
    }

    @Override
    public boolean hasPermission(Permission perm) {
        return permissions().hasPermission(perm);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value) {
        return permissions().addAttachment(plugin, name, value);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin) {
        return permissions().addAttachment(plugin);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value, int ticks) {
        return permissions().addAttachment(plugin, name, value, ticks);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, int ticks) {
        return permissions().addAttachment(plugin, ticks);
    }

    @Override
    public void removeAttachment(PermissionAttachment attachment) {
        permissions().removeAttachment(attachment);
    }

    @Override
    public void recalculatePermissions() {
        permissions().recalculatePermissions();
    }

    @Override
    public Set<PermissionAttachmentInfo> getEffectivePermissions() {
        return permissions().getEffectivePermissions();
    }
}
