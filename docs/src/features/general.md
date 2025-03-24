# General
## AI
Ask the AI assistant how something works and it will explain it to you.
Additionally AI could provide text to speech, and vice versa, audio and video assets, mesh generation, texture generation, and more.

## Drag and Drop
Support as many drag and drop operations as possible that would make sense in the context of the app. Make it easy for it to interact with what's going on in your computer. Drag a picture or a video onto a node that accepts pics or videos. And if there is nothing like that available - then create the node for it automatically.

## Editors
Editors (Node, Environment, Player?) will share functionality - inventory comes to mind. Make sure they work the same in every environment so people can habituate

## Inventory
- In the node editor, add nodes with [command palette](../design/command_palette.md) or from an inventory - maybe both are available. Potentially you could intermix command palette and a visual representation so that when you choose a category you fly (whoosh) to that area - or directly to a searched for item.
- Probably more generally, we're will be inventory management in the various editors so why not make them work the same way

## Persistence
Everything is always saved automatically but you can make a copy at any time. Explicit save is unnecessary. untitled.hana would be the default name for a new file.

## Selection
- when approaching something that is selectable (or if we're using a selection tool a la ultrahand) the closer you get to something selectable, it should give you some kind of visual, audio or haptic feedback.  When I back up my car in the garage, the action is more dangerous so the feedback is an audible beep that gets more frequent as I approach the back wall. Maybe this kind of affordance would be good for doing something that is a little more drastic than just selecting something.
Some deep thought should go into selection for hana - in general.

## Undo/Redo
Undo and Redo are available to the limit possible. Could we make the undo stack searchable? I.e., a list of operations?
