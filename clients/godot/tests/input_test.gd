extends SceneTree


func _initialize() -> void:
	var scene = load("res://scenes/main.tscn").instantiate()
	# Test release handling directly: GUI-consumed releases never reach
	# _unhandled_input, but must still end the gesture through _input.
	scene._panning = true
	var release := InputEventMouseButton.new()
	release.button_index = MOUSE_BUTTON_LEFT
	release.pressed = false
	scene._input(release)
	assert(not scene._panning)
	scene._pan_touch_index = 3
	var touch := InputEventScreenTouch.new()
	touch.index = 2
	touch.pressed = false
	scene._input(touch)
	assert(scene._pan_touch_index == 3)
	touch.index = 3
	scene._input(touch)
	assert(scene._pan_touch_index == -1)
	scene._panning = true
	scene._pan_touch_index = 3
	scene._notification(Node.NOTIFICATION_APPLICATION_FOCUS_OUT)
	assert(not scene._panning and scene._pan_touch_index == -1)
	scene.free()
	print("PASS: pan releases and focus loss reset gesture ownership")
	quit()
