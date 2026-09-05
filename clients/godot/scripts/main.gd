extends Node3D

const MIN_CAMERA_Z := 6.5
const MAX_CAMERA_Z := 15.0

@onready var camera_rig: Node3D = $CameraRig
@onready var camera: Camera3D = $CameraRig/Camera3D
@onready var status_label: Label = $Interface/TopBar/Content/Status

var _focused_panel: SpatialPanel
var _panning := false
var _pan_touch_index := -1


func _ready() -> void:
	$Interface/TopBar/Content/Reset.pressed.connect(_reset_view)
	status_label.text = "Native 3D prototype · protocol bridge pending"


func _input(event: InputEvent) -> void:
	# Observe releases before Controls can consume them in _gui_input.
	if event is InputEventMouseButton and not event.pressed:
		if event.button_index == MOUSE_BUTTON_LEFT:
			_panning = false
	elif event is InputEventScreenTouch and not event.pressed:
		if event.index == _pan_touch_index:
			_pan_touch_index = -1


func _notification(what: int) -> void:
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT:
		_panning = false
		_pan_touch_index = -1


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mouse_event := event as InputEventMouseButton
		if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
			_zoom(-0.7)
		elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
			_zoom(0.7)
		elif mouse_event.button_index == MOUSE_BUTTON_LEFT:
			_panning = mouse_event.pressed and not _focus_at(mouse_event.position)
	elif event is InputEventMouseMotion and _panning:
		var motion := event as InputEventMouseMotion
		if not motion.button_mask & MOUSE_BUTTON_MASK_LEFT:
			_panning = false
			return
		camera_rig.position += Vector3(-motion.relative.x, motion.relative.y, 0.0) * 0.008
	elif event is InputEventScreenTouch:
		var touch := event as InputEventScreenTouch
		if touch.pressed and _pan_touch_index == -1 and not _focus_at(touch.position):
			_pan_touch_index = touch.index
	elif event is InputEventScreenDrag and event.index == _pan_touch_index:
		var drag := event as InputEventScreenDrag
		camera_rig.position += Vector3(-drag.relative.x, drag.relative.y, 0.0) * 0.008


func _focus_at(screen_position: Vector2) -> bool:
	var origin := camera.project_ray_origin(screen_position)
	var target := origin + camera.project_ray_normal(screen_position) * 100.0
	var query := PhysicsRayQueryParameters3D.create(origin, target)
	var hit := get_world_3d().direct_space_state.intersect_ray(query)
	if hit.is_empty():
		_set_focused_panel(null)
		return false
	var candidate := hit["collider"] as Node
	while candidate != null and not candidate.is_in_group("spatial_panels"):
		candidate = candidate.get_parent()
	if candidate is SpatialPanel:
		_set_focused_panel(candidate as SpatialPanel)
		return true
	return false


func _set_focused_panel(panel: SpatialPanel) -> void:
	if _focused_panel != null:
		_focused_panel.set_focused(false)
	_focused_panel = panel
	if panel == null:
		status_label.text = "Native 3D prototype · protocol bridge pending"
		return
	panel.set_focused(true)
	status_label.text = "%s selected" % panel.title_text


func _zoom(delta: float) -> void:
	camera.position.z = clampf(camera.position.z + delta, MIN_CAMERA_Z, MAX_CAMERA_Z)


func _reset_view() -> void:
	camera_rig.position = Vector3.ZERO
	camera.position.z = 10.0
	_set_focused_panel(null)
