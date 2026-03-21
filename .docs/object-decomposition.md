# Object decomposition

Document tree is essentially a recursively nested sorted map - even the indexed sequences are realized as a ranges of
lexically ordered byte strings (see: [LSeq](./lseq.md)).

However, when it comes to a physical representation of the document tree on the disk, we fall back to series of
key-value entries. These entries are build by decomposing document object into collection of paths from the document
root to the leaf. In order to make it space efficient, a key-prefix compression is crucial here.

As an example, we can decompose following JSON document:

```json
{
  "id": "f1b3ab2d-31ad-4973-a56d-97f2154845e5",
  "name": "Arthur Smith",
  "address": {
    "city": "London",
    "street": "King's Road"
  },
  "favouriteTracks": [
    "ef9ca47e-f62e-4985-8022-4f05ef17b850",
    "97569598-7d31-4b7c-8b89-00790ae49a31",
    "0cff9f74-e723-4712-a6f6-1b1f11c516be"
  ]
}
```

into something like:

```peon
id=f1b3ab2d-31ad-4973-a56d-97f2154845e5
name=Arthur Smith
address.city=London
address.street=King's Road
favouriteTracks.A1=ef9ca47e-f62e-4985-8022-4f05ef17b850
favouriteTracks.A2=97569598-7d31-4b7c-8b89-00790ae49a31
favouriteTracks.A3=0cff9f74-e723-4712-a6f6-1b1f11c516be
```

and after key-prefix compression, as:

```peon
0|id=f1b3ab2d-31ad-4973-a56d-97f2154845e5
0|name=Arthur Smith
0|address.city=London
8|street=King's Road
0|favouriteTracks.A1=ef9ca47e-f62e-4985-8022-4f05ef17b850
17|2=97569598-7d31-4b7c-8b89-00790ae49a31
17|3=0cff9f74-e723-4712-a6f6-1b1f11c516be
```

where first number is a number of bytes shared between current key and its predecessor (key-prefix compression), then a
unique path suffix of that entry and finally the value itself. We don't use regular indices for arrays, but byte
strings (here represented by `A1..A3`) that are generated using LSeq sequence generator.

CDoc doesn't support any kind of byte strings as keys. Instead, we use compose them as follows:

- Regular map entries must be a human-readable UTF-8 encoded strings. Human-readable means, that printer control
  characters of ASCII (numbers 0-31) are not allowed. We reuse this range for a special kind of characters.
- Byte `0` is used as a delimiter between path segments.
- Bytes `1..16` are used for LSeq sequences. The first byte marks number of bytes of the prefix-length
  varint-encoded PID. Notice, that LSeq itself can have segments containing `0` bytes inside - it's the nature of varint
  encoding. These are not path segment delimiters.
- Bytes `17..30` are restricted for the future use.
- Byte `31` means that this entry contains chunked content. In that case the next varint bytes after it describe the
  last index of the chunked content. Eg. for object with blob `{ "a": "deadbeef" }`, `a{31}{1}5=deadbe` +
  `a{31}{1}7=ef`. We pick the last index instead of the first one, because lookup for chunk containing index X can catch
  the first following index in a single pass by regular binary search.

Values themselves are formatted as:

- 8 bytes of HLC timestamp for this value (each value is LWW register).
- 4 bytes of PID of the last editor.
- CBOR-encoded value.