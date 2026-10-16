// The holocron of a tier the player holds none of (SJK; see README.md): the face
// drained and dark, lit by the world and a faint light of its own, no glow.
// Made by scripts/holocron_assets.py: edit that, not this.
models/sjk/holocron_locked
{
	q3map_nolightmap
	{
		map models/sjk/holocron_locked
		rgbGen lightingDiffuse
	}
	{
		map models/sjk/holocron_locked
		blendFunc GL_ONE GL_ONE
		rgbGen wave sin 0.16 0 0 0
	}
}
