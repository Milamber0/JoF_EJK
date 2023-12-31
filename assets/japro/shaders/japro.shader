bloodExplosion		// spurt of blood at point of impact
{
	cull disable
	nopicmip
	nomipmaps
	{
		animmap 5 gfx/japro/blood201.tga gfx/japro/blood202.tga gfx/japro/blood203.tga gfx/japro/blood204.tga gfx/japro/blood205.tga
		blendfunc blend
	}
}

bloodTrail
{		
	nopicmip			// make sure a border remains
	entityMergable		// allow all the sprites to be merged together
	{
		//clampmap gfx/misc/blood.tga
		clampmap gfx/damage/blood_spurt.tga
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen		vertex
		alphaGen	vertex
	}
}

bloodMark
{
	nopicmip			// make sure a border remains
	polygonOffset
	{
		clampmap gfx/damage/blood_stain.tga
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen identityLighting
		alphaGen vertex
	}
}

gfx/effects/sabers/RGBGlow
{	
	nopicmip
	notc
	cull twosided
	{
		map gfx/effects/sabers/RGBGlow
		blendFunc GL_ONE GL_ONE
		glow
		
		rgbGen vertex
	}
}

gfx/effects/sabers/RGBCore
{
	nopicmip
	notc
	cull twosided
	{
		map gfx/effects/sabers/RGBCore
		blendFunc GL_ONE GL_ONE
		
		rgbGen vertex
	}
}

gfx/2d/numbers/zero
{
	nopicmip
	{
		map gfx/2d/numbers/zero
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/one
{
	nopicmip
	{
		map gfx/2d/numbers/one
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/two
{
	cull twosided
	nopicmip
	{
		map gfx/2d/numbers/two
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/three
{
	nopicmip
	{
	map gfx/2d/numbers/three
	blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/four
{
	nopicmip
	{
		map gfx/2d/numbers/four
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/five
{
	nopicmip
	{
		map gfx/2d/numbers/five
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/six
{
	nopicmip
	{
		map gfx/2d/numbers/six
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/seven
{
	nopicmip
	{
		map gfx/2d/numbers/seven
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/eight
{
	nopicmip
	{
		map gfx/2d/numbers/eight
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/numbers/nine
{
	nopicmip
	{
		map gfx/2d/numbers/nine
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/2d/minimap
{
	nopicmip
	notc
	q3map_nolightmap
	{
		map gfx/2d/minimap
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
	}
}

gfx/effects/grapple_line
{
	cull twosided
	{
		map gfx/effects/grapple_line
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
		glow
	}
}

gfx/hud/jk2hudleft
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/static5
		blendFunc GL_ONE GL_ONE
		rgbGen wave inversesawtooth 0 1.5 1.4 1
		tcMod scroll 0 1
	}
	{
		map gfx/hud/static8
		blendFunc GL_ONE GL_ONE
		rgbGen vertex

	}
	{
		map gfx/hud/jk2hudleft
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/jk2hudleft_innerframe
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/jk2hudleft_innerframe
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/prong_off
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/prong_off
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/prong_on_weap
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/prong_on_w
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/prong_on_item
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/prong_on_i
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/prong_on_force
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/prong_on_f
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/jk2hudrightframe
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/static5
		blendFunc GL_ONE GL_ONE
		rgbGen wave inversesawtooth 0 1.5 1.4 1
		tcMod scroll 0 1
	}
	{
		map gfx/hud/static9
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
	}
	{
		map gfx/hud/jk2hudrightframe
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/jk2hudright_innerframe
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/jk2hudright_innerframe
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/saber_stylesfast
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/saber_stylesfast
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
	}
}

gfx/hud/saber_stylesmed
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/saber_stylesmed
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
	}
}

gfx/hud/saber_stylesstrong
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/saber_stylesstrong
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
	}
}

gfx/hud/saber_stylesstaff
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/saber_stylesstaff
		blendFunc GL_ONE GL_ONE
		rgbGen vertex
	}
}

gfx/hud/i_icon_healthdisp
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/i_icon_healthdisp.tga
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

gfx/hud/i_icon_ammodisp
{
	nopicmip
	nomipmaps
	notc
	cull disable
	{
		map gfx/hud/i_icon_ammodisp.tga
		blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA
		rgbGen vertex
	}
}

//Shader fix by TnG, adapted to Eternaljk by Art

models/map_objects/imperial/airpure_fixed
{
	{
		map $lightmap
		rgbGen identity
	}
	{
		map models/map_objects/imperial/airpure
		blendFunc GL_DST_COLOR GL_ZERO
		rgbGen identity
	}
	{
		map models/map_objects/imperial/airpure_glw
		blendFunc GL_ONE GL_ONE
		glow
		rgbGen identity
	}
}

models/map_objects/ships/cart_fixed
{
	{
		map $lightmap
		rgbGen identity
	}
	{
		map models/map_objects/ships/cart
		blendFunc GL_DST_COLOR GL_ZERO
		rgbGen identity
	}
}
