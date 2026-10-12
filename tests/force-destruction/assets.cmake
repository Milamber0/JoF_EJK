# Destruction uses stock media when optional local assets are unavailable.
# Do not ship the removed custom media or leave it in the asset source tree.
file(GLOB_RECURSE custom_media LIST_DIRECTORIES false
    "${asset_dir}/effects/forcedestruction/*"
    "${asset_dir}/gfx/forcedestruction/*"
    "${asset_dir}/sound/forcedestruction/*")
if(custom_media)
  message(FATAL_ERROR "Custom Destruction media must not be bundled: ${custom_media}")
endif()
foreach(path IN ITEMS
    effects/force/destruction.efx effects/force/destruction_explode.efx
    gfx/mp/force_destruction.tga sound/weapons/force/destruction.wav
    credits/force-destruction.txt
    effects/destruction/projectile.efx effects/destruction/impact.efx
    effects/jof/destruction/projectile.efx effects/jof/destruction/impact.efx
    shaders/force_destruction.shader
    shaders/firstmovingball.shader shaders/newmovingball.shader
    models/players/movingball/model.glm ext_data/npcs/zzforcedestruction.npc)
  if(EXISTS "${asset_dir}/${path}")
    message(FATAL_ERROR "Retired or excluded Destruction asset: ${path}")
  endif()
endforeach()
message(STATUS "Custom Destruction media and credit are absent; stock fallbacks require no bundled assets.")
