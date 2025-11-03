Theory
======

The stringdex information retrieval pipline goes through the following steps:

1. It generates an implicit suffix tree according to Ukkonen's Algorithm.
   Suffix trees are a trie-based data structure that we can use to find
   substring matches in *O*(n) time.
2. It marks the portion of the suffix tree that's unneeded for full-word
   matching, so that it doesn't need to be loaded during levenshtein search.
3. Then it serializes parts of the tree, hashes them, and uses that hash to
   deduplicate parts of the tree.
4. Then it compresses parts of the tree that are bundled into the same file,
   so that they can use relative offsets instead of absolute hashids.

Background information: tries and suffix trees
----------------------------------------------

Stringdex is based on a complicated [trie][]. A simple trie is a tree of
characters, and you can construct one like this:

 1. Start the `current node` at the special root node, and the `current char`
    at the first character (if the string is empty, one-past-the-end).
 2. Branch on the value of `current char`:
      - If the `current char` is one-past-the-end, you're done. Add the leaf
        as a child of the `current node` and exit.
      - If the `current node` has a child for the `current char`, move
        `current node` to that child. Advance `current char` to the next,
        and repeat step 2.
      - Otherwise, create a child for `current node` with `current char` in it.
        Advance `current char` to next, move `current node` to the child you
        just created, and repeat step 2.

For example, a trie for `{"cat":1}` looks like this:

    ( )
     │
     c
     │
     a
     │
     t
     │
    (1)

Add `{"cow":2,"dog":3}`, and it now looks like this:

         ( )
       ┌──┴──┐
       c     d
     ┌─┴─┐   │
     a   o   o
     │   │   │
     t   w   g
     │   │   │
    (1) (2) (3)

[trie]: https://en.wikipedia.org/wiki/Trie

The power of the trie, compared to a hash table, is that you can find non-exact
matches for your search query. For example, to find every "c" word, walk the
subtree below the "c" node.

The trie implemented by this crate is a bit more complicated, because we don't
just insert the strings, but every *suffix* of the string, making it a
[suffix tree][]. It's the trie you'd get for
`{"cat":1,"at":1,"t":1,"cow":2,"ow":2,"w":2","dog":3,"og":3,"g":3}`:

                         ( )
       ┌─────┬─────┬───┬──┴──┬─────┬───┐
       a     c     d   g     o     t   w
       │   ┌─┴─┐   │   │   ┌─┴─┐   │   │
       t   a   o   o  (3)  g   w  (1) (2)
       │   │   │   │       │   │
      (1)  t   w   g      (3) (2)
           │   │   │
          (1) (2) (3)

[suffix tree]: https://en.wikipedia.org/wiki/Suffix_tree

While an ordinary trie makes it easy to answer the question "what's every word
that starts with 'c'", a suffix tree answers the question "what's every word
that has an 'o' in it".

A suffix tree is a lot bigger than a regular trie. There are several tactics
that this crate uses to keep the size manageable:

Step 1: Efficient construction of a suffix tree
-----------------------------------------------

This crate contains a version of [Ukkonen's algorithm][], which can
generating suffix trees in non-quadratic time.
How is this possible when "w,ow,cow...somewordendingincow" has a total of
1+2+3..+N = [N(N+1)/2][triangular number] characters?

The Wikipedia references, and especially [desmond's blog][],
describe it much better than I do. But the key points are:

- A node can contain more than one byte. This makes it necessary to
  implement complex "splitting" logic, but reduces the per-node overhead
  by having fewer nodes.
- Each node stores offsets into a shared buffer, instead of storing the
  bytes directly. This means that the length of the string associated
  with each node has no effect on how much space the node takes up
  at construction (the serialized form inlines the text for data locality).
- This is a byte trie. So each node has, at most, 256 children.

[Ukkonen's algorithm]: https://en.wikipedia.org/wiki/Ukkonen%27s_algorithm
[triangular number]: https://en.wikipedia.org/wiki/Triangular_number
[desmond's blog]: http://programmerspatch.blogspot.com/2013/02/ukkonens-suffix-tree-algorithm.html

This means that the suffix tree for `{"cat":1,"cow":2,"dog":3}` becomes this:

    Backing buffer: catcowdog

    Tree:
                         ( )
       ┌─────┬─────┬───┬──┴──┬─────┬───┐
      1,2   0,0   6,8 8,8   4,4   2,2 5,5
       │   ┌─┴─┐   │   │   ┌─┴─┐   │   │
      (1) 1,2 4,5 (3) (3) 8,8 5,5 (1) (2)
           │   │           │   │
          (1) (2)         (3) (2)

Notice that `6,8`, the node that used to be the three-node chain `d-o-g`, takes
up the same amount of space as `5,5`, a node representing only `w`.

This code can be found in `src/internals/tree/mod.rs`.

Step 2: The suffixes-only flag
------------------------------

In the function `make_explicit_tree_and_mark_suffixes_only`, which performs
the last step of Ukkonen's algorithm, it also distinguishes suffix leaves
from whole-word leaves, and bubbles up a `suffixes_only` flag on all
branches that transitively contain only suffix leaves.

This results in an ordinary prefix trie interleaved with the
suffix tree. The part of the tree that needs loaded when performing a
prefix-only match is represented with thick lines (because the trees
are interleaved, you can't avoid downloading the list of suffix-only children
of the root node, even when doing a no-suffixes search, but the child nodes
don't need traversed):

    Backing buffer: catcowdog

    Tree:
                         ( )
       ╒═════╦═════╦═══╤══╩══╤═════╤═══╕
      1,2   0,0   6,8 8,8   4,4   2,2 5,5
       │   ╔═╩═╗   ║   │   ┌─┴─┐   │   │
      (1) 1,2 4,5 (3) (3) 8,8 5,5 (1) (2)
           ║   ║           │   │
          (1) (2)         (3) (2)

Step 3: Serialization
---------------------

Once the tree has been constructed, the generator crate needs to store the
tree to disk in a format that the JavaScript-based searcher can read.

While Ukkonen's algorithm is mostly a pure win for generating the tree in the
first place, formatting on disk has more meaningful trade-offs, for latency,
total storage used, and generation speed.

- While the crate uses the "huge shared buffer" for construction, the
  serialized tree stores the text inline.
- It uses a scheme where multiple tree nodes can be stored in a single file.
- The disk format is a merkle tree. That is, a node is named after its hash.
  Each file has a "subtree root node", whose hash becomes the filename.

By using a merkle tree, we get HTTP cache busting "for free," since each
node's file is named after its subtree root hash, which changes if the node's
transitive contents change. We also get de-duplication: in the example above,
the `1,2 -> (1)` and `8,8 -> (3)` and `5,5 -> (2)` subtrees are duplicates.

Each node in the search tree is formatted like this:

  * The first 1/2 bytes is used for compression data. In canonical form,
    it's either 0 or 1. A 1 means the node is pure-suffix-only.
    See "Step 4".

  * Data length, as one byte. This means a single node can only contain
    255 characters of data.

  * Data. The initial character, which our parent node has a copy of,
    is not stored here and is not considered part of the length.

  * Child length counts, one byte each:

    - first might-have-prefix child count
      (that is, children that are not `suffixes_only`),
      or, if the node is pure-suffix-only, this count
      is omitted
    - then `suffixes_only` child count

    These two sets of children are disjoint, which means there can be a maximum
    of 256 children in total between both of them. There is a single byte for
    each count, which can only go to 255.

    If there are 256 children, then I don't retain the suffixes-only
    information: both bytes are set to 255 (which cannot happen otherwise),
    and all children are encoded and decoded as-if any of them could have
    prefix leaves beneath them. This is safe, because prefix matching needs to
    filter out suffixes anyway, and it's not a serious performance problem,
    because UTF8 text (and, I suspect, most data that isn't compressed)
    doesn't use every possible byte.

    An additional special case is used by setting every (or, in the pure-
    suffix-only case, the only) count to a value of 0x80 (128 decimal)
    `OR` the length. This means the children are serialized as a bitmap instead of
    a list of bytes. There is no ambiguity, because that would imply
    more than 256 total children (which is impossible) and bitmaps never have
    more than 32 children in them.

  * Six byte child nodeIDs, first might-have-prefix ones, then the
    suffixes-only ones. If a node is not found in the same file as this node,
    it will be found in a file named after this hash, as hex (12 char).

    As a further optimization, there are two kinds of node IDs. Hashes have
    the MSB set to zero (leaving 47 bits of hash strength). Hash collisions
    are handled by adding a dummy node child and rehashing.

    The other representation is used for nodes that have exactly one leaf
    child, and has the MSB set to one. The second-most-significant-bit
    is a tag marking the node as either a whole match (1) or a suffix (0).
    The third is a tag marking the node as either containing or not containing
    a payload byte. The fourth declares if the node has one or two leaves.
    The remaining data is used to store the byte (if any), followed by the
    leaf or leaves.

    If the node has exactly one leaf, its 32 bit ID is simply stored.
    If it has two, it'll it'll pack a 28 or 20 bit ID for the smaller leaf
    (depending on whether there's a payload byte), followed by a 16 bit offset
    for the bigger leaf. Since either leaf might not fit, we fall back on the
    non-inlined representation if it can't.

  * Child branches, first might-have prefix ones, then the
    suffix-only ones. These are in the same order as the IDs, so to act as
    a set of key-value pairs, but are stored separately for gzipability.
    If the count byte specifies a bitmap, then that's what's used; otherwise,
    a flag list of bytes. The might-have-prefix branches are skipped if the
    node is pure-suffix-only.

  * Roaring bitmaps for whole strings and suffixes.

    The length of the bitmap is calculated while decoding it, so that the
    node is self-terminating.

    Roaring bitmaps start with 32-bit magic number (which may have the
    bitmap's length OR-ed with it). If both bitmaps are empty, we drop
    ff here, which is shorter than encoding two empty sets normally.
    If the bitmap has a small number of entries (less than 0x3a), we drop
    NN here, followed by NN 32-bit words, if it turns out to be a win
    (the magic number that starts a roaring bitmap is either 3a or 3b).

### The list of bytes format

The canonical format, using the same example as always, formats the root node
like this:

    00000000  00 00 02 05 4b db 3e d3  77 46 67 1c cc f3 33 1d  |....K.>.wFg...3.|
    00000010  81 74 00 00 00 00 80 00  00 00 00 02 2b 00 8a 05  |.t..........+...|
    00000020  1f 52 80 00 00 00 00 00  80 00 00 00 00 01 63 64  |.R............cd|
    00000030  61 67 6f 74 77 ff                                 |agotw.|

Walking through it chunk-by-chunk:

    00000000  00 00 02 05 4b db 3e d3  77 46 67 1c cc f3 33 1d  |....K.>.wFg...3.|
              ┌─ ┌─ ┌──── ┌───────────────── ┌────────────────
              │  │  │     │                  │
              │  │  │     └ `c` hash         └ `d` hash
              │  │  │
              │  │  ├ 2 might-have-prefix children
              │  │  └ 5 suffix-only children
              │  │
              │  └ data length (always zero for the root node)
              │
              └ compression metadata and the pure-suffix flag

    00000010  81 74 00 00 00 00 80 00  00 00 00 02 2b 00 8a 05  |.t..........+...|
              ┌──────────────── ┌───────────────── ╭──────────
              │                 │                  :
              │                 └ `g` inlined ID   :
              │                   80 is tag        :
              └ `a` inlined ID    02 is leaf ID    :
                81 is tag                          :
                74 is ascii `t`                    :
                0 is leaf ID                       :
              .. .. .. .. .. .. .. .. .. .. .. .. ..
            :
    00000020: 1f 52 80 00 00 00 00 00  80 00 00 00 00 01 63 64  |.R............cd|
            ╰┬───── ┌────────────────  ┌──────────────── ┌────
             │      │                  │                 │
             │      │                  │                 └ `c` `d`
             │      │                  │
             │      │                  └ `w` inlined ID
             │      │                    80 is tag
             │      │                    1 is leaf ID
             │      │
             │      └ `t` inlined ID
             │        80 is tag
             │        0 is leaf ID
             │
             └ `o` hash

    00000030  61 67 6f 74 77 ff                                 |agotw....|
              ┌───────────── ┌─
              │              │
              │              └ NIL
              │
              └ `a` `g` `o` `t` `w`

These nodes are "bundled" by concatenating them together so that dependencies
come BEFORE the data they depend on. That means the node that a file is named
for is the last node in the file. If a dependency is not found in the file,
it's lazy-loaded as needed.

### The bitmap format

The bitmap format, mentioned above, could have written the root note like this
instead:

    00000000  00 00 82 85 4b db 3e d3  77 46 67 1c cc f3 33 1d  |....K.>.wFg...3.|
    00000010  81 74 00 00 00 00 80 00  00 00 00 02 2b 00 8a 05  |.t..........+...|
    00000020  1f 52 80 00 00 00 00 00  80 00 00 00 00 01 03 00  |.R..............|
    00000030  00 02 02 28 77 ff                                 |.gotw.|

We don't actually use bitmaps in this case, since they aren't a win.

There are actually two bitmap formats; the above example uses the three bit
"short" one, which assigns bits to chars like this:

  |0|1|2|3|4|5|6|7|8|9|a|b|c|d|e|f|
--|-|-|-|-|-|-|-|-|-|-|-|-|-|-|-|-|
 0|a|b|c|d|e|f|g|h|i|j|k|l|m|n|o|p|
 1|r|s|t|u|w|x|y|z|               |

Notice that "q" and "v" are absent, in order to get to exactly 24 letters.
So the final bitmap for the may-have-prefix nodes (that's "cd") is
`00110000 00000000 00000000` in binary, or `30 00 00` hex, and the suffix-only
nodes become `00000010 00000010 00101000` in binary, or `02 02 28`.

There's a four-byte form with the entire alphabet, that's written like this:

  |0|1|2|3|4|5|6|7|8|9|a|b|c|d|e|f|
--|-|-|-|-|-|-|-|-|-|-|-|-|-|-|-|-|
 0|1|2|3|4|5|6|a|b|c|d|e|f|g|h|i|j|
 1|k|l|m|n|o|p|q|r|s|t|u|v|w|x|y|z|

I stuck 1-6 in the gap because this covers a lot of cases where you've got two
otherwise-identical strings that differ in a digit, like "version1" versus "version2",
or "

To flag it, we `OR` the lengths with `0xc0` instead of `0x80`. This gives our
bitflag representations a max length of 32.

### Pure suffix representation

The least significant bit of the first byte is used to mark a node as pure-suffix,
meaning all of its children and leaves are suffix-only leaves, and the data is omitted.

The "data is omitted" part is the complex one. It's a bit inspired by how
[PATRICIA trees][] work, but it's byte-based instead of bit-based, so not quite.
The length is still there, but the payload isn't, so to actually check a match, it
must retrieve the full name from the data column (which rustdoc search has to do
anyway for ranking by position).

[PATRICIA trees]: https://en.wikipedia.org/wiki/Radix_tree#Variants


Step 4: Compression
-------------------

The sample I gave above is the "canonical" form of the root node. This is the
form used for hashing, because it contains the hashids of all its children,
and it contains all the data, so hashing *that* is hashing the entire subtree.

For actual storage, it will, under specific circumstances, use a different
serialization form:

To describe this form, the first byte is encoded like this:

  * The most significant nibble is a list of bits, marking which child nodes are
    stored using offset compression. If the nibble is a 1111, and if the
    "long" flag isn't set, then that means all child nodes are stored using
    offset compression.

  * The MSB of the second nibble is set to 1 if dictionary compression is used.

  * The second bit picks between "long" and "short" list of child node bits.
    If it's long, then the second and third bytes are an additional list of
    bits to declare which children use offset compression.

  * The third bit picks between stack compression and backref compression.
    In stack compression, the pointers are implicit, while backref uses
    one byte to look back to previous nodes in the same file.
  
  * The least significant bit picks between pure-suffix-only and may-have-prefix
    nodes: this is the only bit that is used in the canonical form shown above.
    More information on this above, under the "Pure suffix representation" head.

  * If the "long" bit is set, two more bytes are dropped in with child compression
    flags, just like the most significant nibble of the first byte.

The remaining bytes look like this:

  * Data length, as one byte, or a data offset, also one byte,
    depending on whether dictionary compression is used for the data.

  * Data. The initial character, which our parent node has a copy of,
    is not stored here and is not considered part of the length. Absent
    if dictionary compression is used, or if length is zero.

  * Child length counts. The same format as canonical.

  * The pointers, encoded as either offset or six byte ID, depending on if
    the corresponding flag in the first byte is set. For example, if the
    compression byte is 0x10, then the first child is an offset.

    If stack compression is used, then the decoder will pick the previous
    children in reverse order, skiping any children that have already been
    referred to by another node.

    If backref compression is used, then the nodes are picked by their
    previous position, starting at 0.

  * Child branches, in the same form used by canonical.

  * Roaring bitmaps for whole strings and suffixes, with special representation
    for very short lists. The format is the same as the canonical format.
