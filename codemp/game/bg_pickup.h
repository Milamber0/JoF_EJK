// Capacity-limited pickups confirmed by version 1 of the pickup protocol.
static qboolean BG_ConfirmedPickupType(int type) {
	return type == IT_HEALTH || type == IT_ARMOR ||
		type == IT_AMMO || type == IT_HOLDABLE;
}
