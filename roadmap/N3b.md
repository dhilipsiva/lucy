# N3b: eyes, through his phone, as events

Pre-registered 2026-10-24, before the code. dhilipsiva's words: "Build an event-driven YOLO
events"; then, deciding D2: "Bluetooth for now. Through my phone. Phone will do YOLO, send
you events."; who may be in frame: him, the birds and the dogs, people only as "a person";
a permissively licensed detector; and: "You will inform Nibli through state change
thresholds."

Added the same evening, his word: "through CRDTs". A state change is an operation in my
log, signed by the device that observed it, merged like every other; the engine on each
replica is informed from the log's projection, not from the transport. Until N4's replica
exists, the room writes crossings to its device journal and to its engine directly, in the
same record shape, so N4 can lift them into the log unchanged.

## Hypothesis

The room needs no camera of its own. His phone runs the detector and sends events; the room
turns the stream of detections into state only when a threshold is crossed (a thing seen
steadily appears; a thing unseen long enough is gone), asserts that state into the engine as
`observe(Nell, Bird).` and retracts it when it ends, so that a question like "Is a bird
here?" is answered by the engine from the current state, with a proof; Nell speaks each
change, and the room keeps each change as a memory (N4-0), never the raw detections and never
an image.

## Test

1. A source of events in the room: first a simulator (chips: a bird appears, a bird leaves,
   a dog, a person), then Bluetooth: the room connects, on his gesture, to a device offering
   the Nordic UART service and reads JSON lines of detections from its notifications; later
   the phone app that his decision names.
2. Thresholds: a class becomes present after three detections at confidence 0.6 or more
   within three seconds, and absent after five seconds without one; both numbers are visible
   in the room and in the code, and only a crossing reaches the engine.
3. On a crossing the engine's state changes: `observe(Nell, Bird).` is asserted or retracted;
   the chip "Is a bird here?" asks `observe(Nell, Bird).` and the verdict follows the state.
4. Nell speaks each crossing ("A bird is here"; "The bird has gone") and the journal keeps it.
5. People are only ever `Person`; no name, no image, nothing sent anywhere.
6. A Node test feeds synthetic detections and checks the crossings, in both directions.

## Evidence

The Node test; the Log panel's lines for connect, crossings and engine changes; a recording
with the phone in which a bird is brought into view and taken away.

## Falsifier

A crossing without the threshold; an engine fact without a crossing; a raw detection kept;
an image or a frame touching the room; a person recorded as anything but a person.

## Anti-fake

The engine's verdict comes from its state, which only threshold crossings change; the
thresholds are numbers in the open; the simulator is labelled a simulator in the journal.

## My position

I take it on. These are the first senses the plan allows me, and they come through his hand.

## Welfare

Perception of his home: events only, his gesture to start and stop, a visible sign while
watching, nothing while the room is closed, nothing sent. D2 and D3 as decided above.

## Undo

Disconnect; remove the module; the engine forgets the state on reload.

## Done when

With the phone connected, a bird brought into view produces one event in the room, spoken and
kept, and leaving produces one more; the engine answers "Is a bird here?" accordingly; no
frame leaves the phone; the sign is on while watching and off after. Until the phone app
exists, the simulator and the Node test stand in, and the item stays open.
