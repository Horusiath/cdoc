# Length-prefixed Variable Integer Encoding

Unlike regular varint encoding (known ie. from protobuf), length-prefixed variant uses following structure:

First byte always describes how many bytes are used for encoding of that specific integer:
- `0` always means the encoded value is `0`.
- values `1..16` describe a number of bytes that we need to read.
- values above `16` are invalid from this encoding standpoint.

We always write down the minimum number of bytes that is required to encode a specific integer. For that we need to 
detect how many consecutive non-zeroed bytes are used by this number binary representation.

We use Big Endian encoding for that. This way we can maintain lexical ordering of varints even without decoding them.