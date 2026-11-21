using System.Buffers.Binary;
using Il2CppInspector.Next.Metadata;

namespace Il2CppInspector.Plugin.LimbusCompany;

internal static class LimbusCompanyMetadata
{
    // GlobalMetadata::Initialize (0x1807C01A0) decrypts 0x354 bytes of shuffled section triples.
    // The first payload starts at 0x12EC; stock v39 has sanity/version followed by 31 triples.
    private const int EncryptedHeaderSize = 0x354;
    private const int PayloadOffset = 0x12EC;
    private const int StockHeaderSize = 8 + 31 * 12;
    private const int HeaderDelta = PayloadOffset - StockHeaderSize;

    // ExportedTypeDefinitions ends at 0x31525F8 (0x180806770); the remaining 0xBEC bytes are outside every section.
    private const int TrailerSize = 0xBEC;

    // 0x188FD63E0, indexed by the XOR of the low four bytes of the xorshift64 state.
    private static readonly byte[] KeyTable = Convert.FromHexString(
        "9E0049E77FD2A250999C8BF83840B6FD460F270887BA13CF5C69054F292F038AB81794D31E11EF1C590E655228011FFF79744320735A9F8082D6EC912426F6A8"
            + "D57A89C11DF433D4BEE958125510D91BC247AEBFFC2BC62D48CA4B2CB0F1AD4E8FD7A9AA7BE43EBCDAB9ABE0A73DD88351DF8EDB856776682A75D0C88DC73F09"
            + "5F3719C3610BEA4AFBA5F944A4A6E11862C44202CEAF044C8C15D160F3BB70A0E281C939B35BC07C14164D560CEDDE2E2321A1B2E845E6B19A906F7D9266CDFE"
            + "DC57066CEBE36E976D3AFADDC522B596ACB798773B78416B63F535717EF79D1A939B640DBDEEE55386723425CC3CF2B484885436305D95325E0731A3F06ACB0A"
    );

    internal static byte[] Normalize(ReadOnlySpan<byte> encrypted)
    {
        if (encrypted.Length < PayloadOffset + TrailerSize)
            throw new InvalidDataException($"Limbus Company global-metadata.dat must contain at least {PayloadOffset + TrailerSize} bytes.");
        var header = encrypted[..EncryptedHeaderSize].ToArray();
        Decrypt(header, 0x4A546F7D141D28AC);

        // Header slots and offset corrections come from the readers cited below. Zero stride denotes a blob;
        // its count need not equal its byte length. The seven encrypted payloads use independent seeds (0x1807C01A0).
        (string Name, int Slot, int Correction, int Stride, ulong Seed)[] sections =
        [
            ("StringLiterals", 0x120, -0x1E68, 4, 0xEF53799D5D7DCB45), // 0x1807C0AA0
            ("StringLiteralData", 0x03C, 0x0F90, 0, 0xC01D4AECE717E231),
            ("Strings", 0x2A0, -0x00A8, 0, 0x42AB48AC6E92630C),
            ("Events", 0x09C, 0x1A60, 24, 0), // 0x1807C1DA0
            ("Properties", 0x114, 0x12E0, 20, 0x8DEF87B7BF931EAF), // 0x180817010
            ("Methods", 0x1EC, -0x190C, 32, 0xAD2F7A2CA83276A3), // 0x18080BA50
            ("ParameterDefaultValues", 0x294, 0x03E0, 12, 0), // 0x180806C20
            ("FieldDefaultValues", 0x0FC, -0x1774, 12, 0), // 0x1807C1C10
            ("FieldAndParameterDefaultValueData", 0x21C, -0x0184, 1, 0),
            ("FieldMarshaledSizes", 0x1BC, -0x05F0, 12, 0), // 0x18081B830
            ("Parameters", 0x1C8, -0x1640, 12, 0), // 0x18081BD10
            ("Fields", 0x0E4, -0x1124, 12, 0x4154A2B5BD957E88), // 0x1808156A0
            ("GenericParameters", 0x264, 0x0684, 14, 0), // 0x18080C270
            ("GenericParameterConstraints", 0x078, -0x1F84, 4, 0), // 0x1807C1F60
            ("GenericContainers", 0x018, 0x15C8, 16, 0), // 0x1807C1520
            ("NestedTypes", 0x060, -0x1CF8, 4, 0), // 0x180816700
            ("Interfaces", 0x024, 0x0188, 4, 0), // 0x18081B660
            ("VtableMethods", 0x150, -0x1928, 4, 0), // 0x18081AFF0
            ("InterfaceOffsets", 0x108, 0x0200, 8, 0), // 0x18081B720
            ("TypeDefinitions", 0x00C, -0x1F98, 82, 0), // 0x18080BD90
            ("Images", 0x0A8, -0x0AF8, 36, 0), // 0x18080C140
            ("Assemblies", 0x210, 0x07E0, 68, 0x80A72B60E7F92FA3), // 0x180819680
            ("FieldRefs", 0x1F8, 0x17D8, 8, 0), // 0x1807BFFB0
            ("ReferencedAssemblies", 0x2DC, -0x1D8C, 4, 0), // 0x1807D22D0
            ("AttributeData", 0x348, -0x0370, 0, 0), // 0x1807C1A40
            ("AttributeDataRanges", 0x048, -0x0994, 8, 0), // 0x1807DB8A0
            ("UnresolvedIndirectCallParameterTypes", 0x270, -0x167C, 4, 0), // 0x1807C1110
            ("UnresolvedIndirectCallParameterRanges", 0x0B4, 0x178C, 8, 0),
            ("WindowsRuntimeTypeNames", 0x1E0, 0x1390, 8, 0), // 0x1807C0BE0
            ("WindowsRuntimeStrings", 0x030, -0x13FC, 0, 0),
            ("ExportedTypeDefinitions", 0x228, 0x1BAC, 4, 0), // 0x180806770
        ];

        var payloadEnd = encrypted.Length - TrailerSize;
        var output = new byte[payloadEnd - HeaderDelta];
        encrypted[PayloadOffset..payloadEnd].CopyTo(output.AsSpan(StockHeaderSize));
        BinaryPrimitives.WriteInt32LittleEndian(output, Il2CppGlobalMetadataHeader.ExpectedSanity);
        // 0x1807C01A0 derives compressed type/container/parameter index widths; 0x18080BA50 reads a return parameter token.
        // These are stock v39 records (Unity 6000.3.12f1), with no field permutation after section decryption.
        BinaryPrimitives.WriteInt32LittleEndian(output.AsSpan(4), 39);
        var end = PayloadOffset;
        for (var i = 0; i < sections.Length; i++)
        {
            var (name, slot, correction, stride, seed) = sections[i];
            var offset = (long)Read(slot) + correction;
            var size = Read(slot + 4);
            var count = Read(slot + 8);
            if (offset != ((end + 3L) & ~3L) || offset > payloadEnd || size < 0 || size > payloadEnd - offset || count < 0)
                throw new InvalidDataException(
                    $"Limbus Company {name} has an invalid range or count; expected the next aligned section at 0x{end:X}."
                );
            if ((stride != 0 && (long)count * stride != size) || (stride == 0 && count > size))
                throw new InvalidDataException($"Limbus Company {name} has an invalid record size or blob count (size {size}, count {count}).");
            end = (int)offset + size;
            var normalizedOffset = (int)offset - HeaderDelta;
            var entry = output.AsSpan(8 + i * 12, 12);
            BinaryPrimitives.WriteInt32LittleEndian(entry, normalizedOffset);
            BinaryPrimitives.WriteInt32LittleEndian(entry[4..], size);
            BinaryPrimitives.WriteInt32LittleEndian(entry[8..], count);
            if (seed != 0)
                Decrypt(output.AsSpan(normalizedOffset, size), seed);
        }
        if (end != payloadEnd)
            throw new InvalidDataException("Limbus Company metadata sections must cover the complete payload.");

        // v39 literals store count + 1 offsets; the final offset bounds the last string (0x1807C0AA0).
        var literals = output.AsSpan(StockHeaderSize, Read(0x124));
        var literalDataSize = Read(0x40);
        if (literals.Length < 4 || Read(0x128) != Read(0x44) + 1 || BinaryPrimitives.ReadInt32LittleEndian(literals) != 0)
            throw new InvalidDataException("Limbus Company StringLiterals must start at zero and include a final sentinel offset.");
        var previous = 0;
        for (var i = 4; i < literals.Length; i += 4)
        {
            var offset = BinaryPrimitives.ReadInt32LittleEndian(literals[i..]);
            if (offset < previous || offset > literalDataSize)
                throw new InvalidDataException($"Limbus Company string literal {i / 4 - 1} has an invalid data range.");
            previous = offset;
        }
        if (previous != literalDataSize)
            throw new InvalidDataException("Limbus Company final string literal offset must equal StringLiteralData size.");
        var stringOffset = Read(0x2A0) - 0xA8 - HeaderDelta;
        var stringSize = Read(0x2A4);
        if (stringSize == 0 || output[stringOffset + stringSize - 1] != 0)
            throw new InvalidDataException("Limbus Company Strings must end with a null terminator.");
        return output;

        int Read(int slot) => BinaryPrimitives.ReadInt32LittleEndian(header.AsSpan(slot, 4));
    }

    private static void Decrypt(Span<byte> data, ulong state)
    {
        // GlobalMetadata::Initialize at 0x1807C01A0: xorshift64 (13, 7, 17), restarted for each section.
        var table = KeyTable;
        for (var i = 0; i < data.Length; i++)
        {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            data[i] ^= table[(byte)(state ^ (state >> 8) ^ (state >> 16) ^ (state >> 24))];
        }
    }
}
