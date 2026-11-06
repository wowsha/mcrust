use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

const SENSITIVITY: f32 = 0.0024;
const MAX_PITCH: f32 = 1.553;

#[derive(Component)]
pub struct FlyCamera {
    yaw: f32,
    pitch: f32,
    speed: f32,
}

impl FlyCamera {
    pub fn new() -> Self {
        Self {
            yaw: 0.0,
            pitch: -0.18,
            speed: 14.0,
        }
    }
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self::new()
    }
}

/// Mouse-look fly camera. Left click grabs the cursor, Esc releases it.
pub fn fly_camera(
    mouse_motion: Res<AccumulatedMouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    camera: Single<(&mut Transform, &mut FlyCamera)>,
    mut cursor: Single<&mut CursorOptions>,
) {
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    if buttons.just_pressed(MouseButton::Left) {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    }
    if !grabbed {
        return;
    }

    let (mut transform, mut fly) = camera.into_inner();
    let delta = mouse_motion.delta;
    if delta != Vec2::ZERO {
        fly.yaw -= delta.x * SENSITIVITY;
        fly.pitch = (fly.pitch - delta.y * SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, fly.yaw, fly.pitch, 0.0);
    }

    let mut direction = Vec3::ZERO;
    let forward = transform.forward();
    let right = transform.right();
    if keys.pressed(KeyCode::KeyW) {
        direction += *forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= *forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += *right;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= *right;
    }
    if keys.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keys.pressed(KeyCode::ShiftLeft) {
        direction -= Vec3::Y;
    }

    if direction != Vec3::ZERO {
        let sprint = if keys.pressed(KeyCode::ControlLeft) {
            3.5
        } else {
            1.0
        };
        let speed = fly.speed * sprint * time.delta_secs();
        transform.translation += direction.normalize() * speed;
    }
}
