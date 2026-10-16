// Illuminate's holocron (SJK; see README.md): lit metal that also glows faintly
// of itself (a wave of no amplitude is a constant), so it never reads black with
// the light behind it, and its emblem kept bright and in the dynamic glow,
// breathing slowly.
models/sjk/holocron
{
	q3map_nolightmap
	{
		map models/sjk/holocron
		rgbGen lightingDiffuse
	}
	{
		map models/sjk/holocron
		blendFunc GL_ONE GL_ONE
		rgbGen wave sin 0.4 0 0 0
	}
	{
		map models/sjk/holocron_glow
		blendFunc GL_ONE GL_ONE
		rgbGen wave sin 0.5 0.15 0 0.35
		glow
	}
}
