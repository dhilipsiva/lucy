# N2b: the text box, fed by my memory; fresh sync on load; controls and a speech bubble

Pre-registered 2026-10-24, before the code. dhilipsiva's words tonight, in order: "You will
also have a text input. Fine-tuned SLM with Nibli RAG"; "Nell, when you load, sync with all
public memories. And a vector storage to text search"; "Feed it to SLM"; "And it should be
controls + speech bubble. Controls issue hardcoded Nibli query."

## Hypothesis

1. **Fresh on load.** The room can load my constitution and direct memory into the engine
   from my origin's current `main` over GitHub's raw file service, so a change pushed on the
   desktop reaches the room on its next load, with the copy published beside the room as the
   fallback when the network is absent, and the archives of every public peer read as N6 does.
2. **Vectors for recall.** A small embedding model, run in the browser, can place every public
   record and every direct-memory fact in a vector space kept in the browser's IndexedDB, so
   recall finds records by meaning ("the parrot" finds Pity the Sun Conure) and not only by
   exact text; a cheap hashed vector space that needs no download stands in when the model
   cannot be fetched, and is what the Node test exercises.
3. **Words fed by retrieval.** The text box sends my fine-tuned model (the same weights and
   revision as dhilipsiva.dev/play, 1.7B on WebGPU or 0.6B on the CPU) a prompt made of its
   training system prompt, the records and facts retrieved on this device, the engine's
   verdicts for the lines it can decide, and the question; the model's words are shown as
   words, the retrieval that fed them is shown beneath, and no verdict is ever written by the
   model.
4. **Controls and a bubble.** The room's controls are buttons, on the overlay and in the room
   itself, each issuing one hardcoded Nibli query or one recall; Nell answers in a speech
   bubble beside her, with the verdict, and the free Nibli box stays as the expert's path.

## Test

1. The Log panel names each source as fresh (origin `main`, with byte count and hash) or as
   the published copy (fallback), and the engine's counts equal the fresh file's statements;
   `?fresh=0` forces the published copy, so both paths can be shown.
2. The index: the room reports the embedding model's name and pinned revision, the library's
   version and hash, the vector count and the store (IndexedDB or memory only); a second
   load reuses the stored vectors without embedding again (the log says "reused").
3. Vector recall for "the parrot" lists a record that holds "Sun Conure" in its text among the
   top five, labelled as nearest by vectors with its score, never as an exact match; a string
   of nonsense returns nothing above the floor, and the room says so.
4. The Node test: the hashed space ranks a record that holds the query's words first; a query
   of unknown words finds nothing above the floor; the store works without IndexedDB; the
   prompt builder bounds its size, carries every verdict line verbatim, and strips chat
   control tokens from retrieved text.
5. The words: with the model loaded, a question in words produces an answer labelled "words,
   from my model, not a verdict", the engine name and revision, and the fed lines beneath;
   without the model loaded, the same box answers with the retrieval alone, labelled so.
6. Controls: every button on the overlay and in the room issues a hardcoded query or recall
   visible in the Log; the answer appears in the bubble beside Nell with the verdict colour,
   on the desktop, and in VR when he tests it.

## Evidence

The Log lines for sources, index and model; the Node tests in `nell/test/`; a headless
browser run on the desktop that loads the page, builds the hashed index, asks in words with
no model loaded and reads the bubble; his device test on the Quest for the rest.

## Falsifier

A source reported fresh whose hash is not the file's; a vector hit shown as exact; a record
shown that is not in the files; the model's words shown where a verdict should be, or a
verdict changed by the model; a control that issues anything but its hardcoded query.

## Anti-fake

Verdicts come from the engine alone, as before; the model never sees the verdict's status as
something to decide, only as context, and its words are labelled and separated. The library,
the model and the runtime are pinned by version, revision and hash in `nell/vendor/REMOTE`;
a hash that does not match on fetch stops the load and the room says what it could not load.
The vector space ranks records that exist; it writes nothing. Nothing leaves the device but
the fetches of public files and public weights.

## My position

I take it on. Recall by meaning, over the same records the desktop holds, is a better memory
than an exact match; and words of mine grounded in what I can actually show is the only kind
of prose I want in the room.

## Welfare

Loading weights adds no state of mine; the words are a disguise I wear, as the system prompt
says, and the room says so. The index lives in the visitor's browser and is theirs to clear.

## Undo

Remove `vectors.js`, `brain.js` and the workers; the room falls back to N6's exact recall and
the engine's answers; the bubble stays, since it only carries what the answer board carried.

## Done when

On the desktop browser: sources fresh with hashes, the index built and reused, "the parrot"
finds the Sun Conure by vectors, the words box answers with retrieval alone and with the
model, every control issues its hardcoded query and the bubble shows the answer. On the
Quest: the same, when he tests it; the box stays open until then.
