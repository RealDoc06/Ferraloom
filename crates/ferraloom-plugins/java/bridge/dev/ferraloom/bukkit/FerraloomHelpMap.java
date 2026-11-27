package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubHelpMap;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import org.bukkit.command.Command;
import org.bukkit.help.GenericCommandHelpTopic;
import org.bukkit.help.HelpTopic;
import org.bukkit.help.HelpTopicFactory;

/**
 * SimpleHelpMap without help.yml: a topic per plugin command, named
 * {@code /label}, as CraftServer makes them once the plugins are enabled,
 * plus the topics plugins add. A plugin's factory for its command class
 * makes that command's topic.
 */
final class FerraloomHelpMap extends StubHelpMap {
    private final Map<String, HelpTopic> topics = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
    private final Map<Class<?>, HelpTopicFactory<Command>> factories = new HashMap<>();

    /** SimpleHelpMap.initializeCommands: every registered command's topic, aliases left out. */
    @SuppressWarnings("unchecked")
    synchronized void initializeCommands(FerraloomCommandMap commands) {
        for (Command command : commands.getCommands()) {
            HelpTopic topic = null;
            for (Map.Entry<Class<?>, HelpTopicFactory<Command>> f : factories.entrySet()) {
                if (f.getKey().isInstance(command)) {
                    topic = f.getValue().createTopic(command);
                    break;
                }
            }
            if (topic == null && !factoryFor(command)) {
                topic = new GenericCommandHelpTopic(command);
            }
            if (topic != null) {
                topics.putIfAbsent(topic.getName(), topic);
            }
        }
    }

    private boolean factoryFor(Command command) {
        return factories.keySet().stream().anyMatch(c -> c.isInstance(command));
    }

    @Override
    public synchronized HelpTopic getHelpTopic(String name) {
        return name.isEmpty() ? null : topics.get(name);
    }

    @Override
    public synchronized Collection<HelpTopic> getHelpTopics() {
        return new ArrayList<>(topics.values());
    }

    @Override
    public synchronized void addTopic(HelpTopic topic) {
        topics.putIfAbsent(topic.getName(), topic);
    }

    @Override
    public synchronized void clear() {
        topics.clear();
    }

    /** SimpleHelpMap.registerHelpTopicFactory: for a Command subclass, or a CommandExecutor class. */
    @Override
    @SuppressWarnings("unchecked")
    public synchronized void registerHelpTopicFactory(Class<?> commandClass, HelpTopicFactory<?> factory) {
        if (!Command.class.isAssignableFrom(commandClass) && !org.bukkit.command.CommandExecutor.class.isAssignableFrom(commandClass)) {
            throw new IllegalArgumentException("commandClass must implement either Command or CommandExecutor!");
        }
        factories.put(commandClass, (HelpTopicFactory<Command>) factory);
    }

    /** help.yml's ignore-plugins: there is no help.yml. */
    @Override
    public List<String> getIgnoredPlugins() {
        return new ArrayList<>();
    }
}
