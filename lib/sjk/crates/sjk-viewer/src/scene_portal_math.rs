//! codemp CG_Portal / R_GetPortalOrientations / R_MirrorPoint, with a
//! right-handed offscreen camera and explicit mirror sampling inversion.
use glam::{Mat3, Mat4, Quat, Vec3};
use sjk_protocol::EntityState;

/// Copy only the portal-camera fields; caching a network entity would clone its heap storage.
#[derive(Clone, Copy)]
pub(super) struct Portal {
    /// Authored surface marker origin used for plane association.
    pub origin: Vec3,
    remote: Vec3,
    direction: u8,
    roll: f32,
    speed: f32,
    rotating: bool,
}

impl Portal {
    /// Adapt CG_Portal's snapshot fields without retaining or allocating a network state.
    pub fn from_entity(entity: &EntityState) -> Self {
        Self {
            origin: Vec3::from_array(entity.trajectory_base()),
            remote: Vec3::from_array(entity.origin2()),
            direction: entity.event_parameter(),
            roll: f32::from(entity.client_num()) / 256.0 * 360.0,
            speed: entity.integer_field(83).unwrap_or(0) as f32,
            rotating: entity.powerups() != 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    pub matrix: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub pvs: Vec3,
    pub clip_point: Vec3,
    pub clip_normal: Vec3,
    pub mirror: bool,
}

/// Stock perpendicular basis chooses the coordinate axis least parallel to n.
fn perpendicular(n: Vec3) -> Vec3 {
    let abs = n.abs();
    let axis = if abs.x <= abs.y && abs.x <= abs.z {
        Vec3::X
    } else if abs.y <= abs.z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    (axis - n * n.dot(axis)).normalize_or_zero()
}

/// Transform one world-space view through the matching stock portal basis.
pub(super) fn portal(
    normal: Vec3,
    distance: f32,
    portal: &Portal,
    parent_view: Mat4,
    time: i32,
) -> Option<View> {
    let source_origin = portal.origin;
    if (normal.dot(source_origin) - distance).abs() > 64.0 {
        return None;
    }
    let remote_origin = portal.remote;
    let left = perpendicular(normal);
    let surface_axes = Mat3::from_cols(normal, left, normal.cross(left));
    let mirror = source_origin == remote_origin;
    let (surface_origin, camera_origin, camera_axes) = if mirror {
        let point = normal * distance;
        (
            point,
            point,
            Mat3::from_cols(-normal, left, normal.cross(left)),
        )
    } else {
        let forward = Vec3::from_array(sjk_client::legacy_byte_to_direction(portal.direction));
        if forward.length_squared() < 0.5 {
            return None;
        }
        let left = -perpendicular(forward);
        let mut axes = Mat3::from_cols(-forward, -left, forward.cross(left));
        let roll = portal.roll;
        // Read-only established protocol netfield 83: entityState.frame.
        let speed = portal.speed;
        let angle = if portal.rotating {
            if speed != 0.0 {
                time as f32 * 0.001 * speed
            } else {
                roll + (time as f32 * 0.003).sin() * 4.0
            }
        } else {
            roll
        };
        axes.y_axis = Quat::from_axis_angle(axes.x_axis, angle.to_radians()) * axes.y_axis;
        axes.z_axis = axes.x_axis.cross(axes.y_axis);
        (
            source_origin - normal * (normal.dot(source_origin) - distance),
            remote_origin,
            axes,
        )
    };
    let transform = camera_axes * surface_axes.transpose();
    let parent = parent_view.inverse();
    let eye = camera_origin + transform * (parent.w_axis.truncate() - surface_origin);
    let forward = transform * (-parent.z_axis.truncate());
    let up = transform * parent.y_axis.truncate();
    let matrix = glam::camera::rh::view::look_at_mat4(eye, eye + forward, up);
    Some(View {
        matrix,
        eye,
        forward,
        pvs: remote_origin,
        clip_point: camera_origin,
        clip_normal: -camera_axes.x_axis,
        mirror,
    })
}

/// Reflect a camera about a geometric plane without requiring a network portal marker.
pub(super) fn reflect_plane(normal: Vec3, distance: f32, parent_view: Mat4, pvs: Vec3) -> View {
    let parent = parent_view.inverse();
    let reflect = |v: Vec3| v - 2. * normal * normal.dot(v);
    let origin = parent.w_axis.truncate();
    let eye = origin - 2. * normal * (normal.dot(origin) - distance);
    let forward = reflect(-parent.z_axis.truncate());
    let up = reflect(parent.y_axis.truncate());
    View {
        matrix: glam::camera::rh::view::look_at_mat4(eye, eye + forward, up),
        eye,
        forward,
        pvs,
        clip_point: normal * distance,
        clip_normal: normal,
        mirror: true,
    }
}
