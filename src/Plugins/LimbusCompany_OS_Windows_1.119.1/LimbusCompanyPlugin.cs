using Il2CppInspector.Plugins;

namespace Il2CppInspector.Plugin.LimbusCompany;

/// <summary>Decodes Limbus Company 1.119.1 metadata for the stock Unity 6000.3 IL2CPP v39 pipeline.</summary>
public sealed class LimbusCompanyPlugin : GamePlugin
{
    public override string GameId => "LimbusCompany_OS_Windows_1.119.1";

    // The encrypted header includes this build's section offsets, sizes and counts (0x1807C01A0).
    public override bool Matches(ReadOnlySpan<byte> metadata) =>
        metadata.Length == 51720676
        && metadata[..16].SequenceEqual<byte>([0x59, 0x37, 0x90, 0x06, 0x77, 0x20, 0x83, 0x32, 0x90, 0x79, 0xCB, 0xE7, 0xBA, 0xBD, 0x53, 0x3C]);

    public override Il2CppInspector Load(Stream binary, byte[] metadata, byte[] startupMetadata, LoadOptions options, EventHandler<string> status)
    {
        status?.Invoke(this, $"Decoding {GameId} global-metadata.dat");
        using var stream = new MemoryStream(LimbusCompanyMetadata.Normalize(metadata), writable: false);
        var decoded = Metadata.FromStream(stream, status);
        var image = FileFormatStream.Load(binary, options, status) ?? throw new InvalidDataException("Unsupported executable file format.");
        // XqMlxvuLSDi at 0x180823A30 is il2cpp_init and calls Runtime::Init at 0x1807B7820.
        if (image.Arch != "x64" || !image.GetExports().Any(static e => e.Name == "XqMlxvuLSDi"))
            throw new NotSupportedException("Limbus Company 1.119.1 requires its x64 GameAssembly.dll with export XqMlxvuLSDi.");

        // Runtime::Init installs stock registrations at 0x187E51CA0 and 0x188D0F530.
        // Class construction (0x1807C1520) and method setup (0x180816150) retain stock v39 layouts.
        var il2cppBinary =
            Il2CppBinary.Load(image, decoded, status)
            ?? throw new InvalidDataException($"Could not locate the {GameId} IL2CPP registration structures in GameAssembly.dll.");
        return new Il2CppInspector(il2cppBinary, decoded);
    }
}
