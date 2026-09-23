# Native dialogue system

JoF dialogues are server-authoritative graphs stored in `dialogues/*.dlg` inside a loaded PK3. They do not use Lua. The cgame only displays the current node and sends the selected choice index; the game module validates the session, choice, condition, and transition.

## Starting a dialogue

Add a logical entity to a map or entity override:

```
{
    "classname" "target_dialogue"
    "targetname" "quartermaster_talk"
    "dialogue" "example_quest"
    "target" "dialogue_finished"
}
```

Activate `quartermaster_talk` from a trigger, button, or an ICARUS `use` operation. The activating entity must be a player. The optional `target` is fired only when the player completes the graph; pressing Escape cancels without firing it.

An NPC spawner can also start a dialogue directly when the player uses the spawned NPC:

```
{
    "classname" "NPC_Stormtrooper"
    "NPC_type" "stormtrooper"
    "dialogue" "example_quest"
}
```

Map logic can advance a quest without opening a dialogue:

```
{
    "classname" "target_queststage"
    "targetname" "supplies_crate_found"
    "quest" "missing_supplies"
    "stage" "2"
    "target" "supplies_objective_complete"
}
```

Activate it with the player as activator from a trigger, item script, or ICARUS. Its optional `target` fires after the stage is set.

For local testing, enable cheats and run:

```
/dialoguetest example_quest
```

## File format

```
dialogue {
    start first_node

    node first_node {
        speaker "NPC name"
        text "Dialogue text. Use \\n for an explicit line break."

        choice {
            text "Visible answer"
            next second_node
            require quest_id eq 0
        }
    }

    node second_node {
        speaker "NPC name"
        text "This node changes state and activates a map target."
        setquest quest_id 1
        fire map_target_name
        next final_node
    }

    node final_node {
        speaker "NPC name"
        text "Finished."
        end
    }
}
```

Node fields:

- `speaker "text"` and `text "text"` set the presentation.
- `choice { ... }` adds a player answer. Up to twelve choices may be visible. The client shows five at a time and scrolls the response list with the current selection.
- `next node_id` produces an automatic Continue choice when there are no visible explicit choices.
- `end` produces a Close choice when there are no visible explicit choices.
- `setquest quest_id stage` sets that player's integer quest stage.
- `fire targetname` activates existing map targets immediately when the node opens.

Choice fields:

- `text "answer"` is required.
- `next node_id` transitions to another node. Omitting it completes the dialogue.
- `require quest_id operator value` conditionally exposes the choice. Operators are `eq`, `ne`, `lt`, `le`, `gt`, and `ge` (the symbolic forms also work).

Quest stage 0 is implicit. The current implementation intentionally keeps quest stages in memory for the current connection and map. Account persistence should be added at the `DLG_GetQuest`/`DLG_SetQuest` boundary if persistent quests are needed; dialogue files and client protocol do not need to change.

## Why this design

- `.dlg` files suit content iteration and use the engine's existing tokenizer and PK3 filesystem.
- `target_dialogue`, `fire`, and the completion target compose with vanilla triggers and ICARUS instead of replacing them.
- `target_queststage` lets normal map objectives update per-player quest state.
- Conditions and actions stay on the server, so a modified client cannot select a hidden or stale choice.
- Native fixed limits keep parsing and network messages bounded.

See `assets/jofclient/dialogues/example_quest.dlg` for a complete example.
