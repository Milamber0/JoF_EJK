#============================================================================
# Copyright (C) 2013 - 2018, OpenJK contributors
#
# This file is part of the OpenJK source code.
#
# OpenJK is free software; you can redistribute it and/or modify it
# under the terms of the GNU General Public License version 2 as
# published by the Free Software Foundation.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, see <http://www.gnu.org/licenses/>.
#============================================================================

# Monocypher 4.0.2 (https://monocypher.org, BSD-2-Clause or CC0, see LICENCE.md),
# unmodified. The client's updater uses its Ed25519 signature check and SHA-512
# (codemp/client/cl_update.cpp).
#
# It is built as a static relocatable library, and linked into any target that
# requires it.
add_library(bundled_monocypher STATIC monocypher.c monocypher-ed25519.c)

set(MONOCYPHER_LIBRARIES bundled_monocypher PARENT_SCOPE)
set(MONOCYPHER_INCLUDE_DIRS ${CMAKE_SOURCE_DIR}/lib/monocypher/ PARENT_SCOPE)

mark_as_advanced(MONOCYPHER_LIBRARIES MONOCYPHER_INCLUDE_DIRS)

set_property(TARGET bundled_monocypher PROPERTY POSITION_INDEPENDENT_CODE True)
