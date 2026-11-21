# How the documentation of `puns` is written

This file is the standard for every piece of prose in the repository: the README, the `///` and
`//!` documentation, the `// SAFETY:` comments, and the Markdown files. A review of the
documentation measures it against these rules. The model is the documentation of `core`, `std` and
`zerocopy`: read a few of their items before writing or reviewing, such as `core::ptr::read`,
`core::slice::from_raw_parts`, `core::mem::transmute`, `zerocopy::TryFromBytes` and
`FromBytes::ref_from_bytes`.

A review reads each text whole and checks it against every rule of this file. A script that counts
colons, short sentences or contractions only points at places to read, and its numbers are not a
verdict.

## What every text does

- **Each claim is true of the code.** A sentence that the code does not support is a defect,
  however well it reads. Numbers, names and messages are copied from their source, never
  reconstructed.
- **The main thing is said once.** Say it where the reader needs it, and link to the place that
  gives the details. Qualifiers that only a few readers need belong in the linked text.
- **Motivation comes before mechanism.** Say what a thing is for, then how it works.
- **The text has judgment.** It says when to use a thing and when not to, what is a mistake, and
  what is recommended.
- **The same word names the same thing,** everywhere. A term of art (proven, checked, refused,
  rule, view, region) keeps its meaning across files. A pun *reinterprets* bytes as another type,
  as `core` says of `transmute`, while a check or a rule *reads* them.

## How sentences read

- **Common words only.** Use the words that most readers know, including those who read English as
  a second language. A Rust term is fine if `core`, `std` or `zerocopy` uses it. A rarer word
  ("struck", "salient", "elided") is replaced by a common one ("impressed", "main", "left out").
- **No idioms or figures of speech.** "A string of prototypes", "close to landing" and "put it
  aside" become "many prototypes", "about to be stabilized" and "stopped working on it".
- **The simplest sentence that says it.** Write each sentence from what it has to say, not by
  rearranging the old one. "But by then compile-time evaluation was too limited" is better than
  "It could not go far enough. Compile-time evaluation was too limited".
- **Sentences vary in length.** A short sentence states a fact. A longer one carries the reasoning
  that connects facts. A run of short sentences of the same shape reads as generated.
- **Connectives carry the logic, and only the logic.** Join two facts with "because", "so",
  "but" or "while" where one explains, follows from or limits the other. Do not join sentences only
  to make them longer: a sentence of clauses chained by commas reads as generated too.
- **Keep only what answers the question.** A list of what made something possible names only
  what it could not do without, not everything that helped.
- **Possessives and compound nouns replace chains of "of".** "The tag's position" or "the field
  type", not "the position of the tag of the variant of the enum", where the result stays clear.
- **Plain technical words replace circumlocutions.** "Zero-terminated string", "variable-length
  integer" and "pointer method" are clearer than descriptions built from simple words.
- **Examples are concrete.** A real format, real bytes, a real error message.

## Examples the author approved

The History section of the README, before:

> Compile-time evaluation had many limitations. I tried types instead, and found even more. I wrote
> many prototypes that mixed the two approaches. Then I stopped the project for a long time.

A first rewrite, refused for "too much comma":

> Compile-time evaluation had many limitations, and when I tried types instead, I found even more.
> After many prototypes that mixed the two approaches, I stopped the project for a long time.

A second, refused for a word that is not common ("I had been struck by") and for "It could not go
far enough", where a simpler sentence says the same. The text the author approved:

> But by then compile-time evaluation was too limited, types were worse, and after many prototypes
> that mixed the two I stopped working on the project for a long time.

## What reads as generated

Avoid these patterns, each of which is common in generated text:

- A sentence that ends with ", not Y" to contrast with something no one claimed.
- Clause after clause joined by colons, or by commas.
- Rare words and idioms where a common word would do.
- Bold lead words on every bullet, where the bullets are not a catalogue.
- Every paragraph or section ending with a caveat.
- Lists of three by habit.
- The same hedge on every claim.
- Uniform sentence length.
- Em dashes and en dashes.
- Narrating the obvious ("This function does what it says").

## Two registers

**The README** speaks to a person deciding whether to use the crate. It uses "you", contractions
("doesn't", "it's", "can't") and a conversational rhythm, like the READMEs of `anyhow` and
`serde_json`. The History section is the author's account, and its facts are the author's: an
edit changes how it reads, never what it says.

**The reference documentation** (`///`, `//!`, `docs/*.md`) speaks as `core` and `zerocopy` do:

- Negations are mostly written out ("does not", "cannot", "is not"). In the documentation of
  `zerocopy` 0.8.63, under a tenth of negations are contracted, and in `core` between a tenth and
  a fifth, depending on what is counted.
- "You" is used sparingly, two to four times per thousand words in both.
- An item starts with a one-line summary in the third person ("Returns...", "Reinterprets...").
  A description follows, then the sections that apply (`# Errors`, `# Panics`, `# Safety`,
  `# Compile-time errors`), with `# Examples` last.
- An introduction to an example ends with a colon, as in "To fix this pun, declare the padding as
  a field:".

## `// SAFETY:` comments and `# Safety` sections

- **A `# Safety` section lists each requirement as a condition the caller can check,** as
  `core::ptr::read` does: "`src` must be valid for reads", "`src` must be properly aligned". It
  says what happens otherwise only if that is not undefined behavior.
- **A `// SAFETY:` comment states why each precondition of the unsafe operation holds,** one by
  one, as `zerocopy` does. It names the operation's preconditions, not the general purpose of the
  code.
- **A guarantee of Rust is cited, not paraphrased.** Cite the Reference or the documentation of
  `core` by a versioned link and quote the sentence:

  ```rust
  // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align:
  //
  //     Each field lives at byte offset 0 from the beginning of the union.
  ```

- **A function is `unsafe` if its body does something unsafe** that its caller must make sound,
  not because its result matters to safety.

## Mechanics

- Lines are at most 100 columns. Quoted compiler messages, link definitions, table rows and
  commands in code blocks may be longer, since they cannot be wrapped.
- Every link resolves. A link to a hidden item is a dead link, so name such an item in code
  without a link.
- No arbitrary identifiers (rule numbers, labels of review findings, internal ticket ids) in the
  documentation or in commit messages, whose subjects become the changelog. Name a thing by what
  it is.
- Doc tests compile and pass, `cargo doc` builds with `-D warnings`, and an example states its
  result with `assert_eq!` rather than in a comment.
