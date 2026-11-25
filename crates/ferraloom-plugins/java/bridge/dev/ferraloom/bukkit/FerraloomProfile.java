package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubPlayerProfile;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import org.bukkit.profile.PlayerProfile;

/** CraftPlayerProfile without textures: a UUID and a name, either may be missing. */
final class FerraloomProfile extends StubPlayerProfile {
    private final UUID id;
    private final String name;

    FerraloomProfile(UUID id, String name) {
        if (id == null && (name == null || name.isBlank())) {
            throw new IllegalArgumentException("Name and ID cannot both be blank");
        }
        this.id = id;
        this.name = name;
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    @Override
    public String getName() {
        return name;
    }

    @Override
    public boolean isComplete() {
        return id != null && name != null && !name.isEmpty();
    }

    /** Answers the profile as it is: Ferraloom fetches no textures. */
    @Override
    public CompletableFuture<PlayerProfile> update() {
        return CompletableFuture.completedFuture(clone());
    }

    @Override
    public PlayerProfile clone() {
        return new FerraloomProfile(id, name);
    }

    @Override
    public Map<String, Object> serialize() {
        Map<String, Object> out = new LinkedHashMap<>();
        if (id != null) {
            out.put("uniqueId", id.toString());
        }
        if (name != null) {
            out.put("name", name);
        }
        return out;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof FerraloomProfile p && Objects.equals(p.id, id) && Objects.equals(p.name, name);
    }

    @Override
    public int hashCode() {
        return Objects.hash(id, name);
    }

    @Override
    public String toString() {
        return "FerraloomProfile[uniqueId=" + id + ", name=" + name + "]";
    }
}
