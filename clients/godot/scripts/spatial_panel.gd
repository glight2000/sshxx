class_name SpatialPanel
extends Node3D

@export var title_text: String = "Panel"
@export_multiline var body_text: String = ""
@export var accent_color: Color = Color("8b7cff")

@onready var surface: CSGBox3D = $Surface
@onready var title_label: Label3D = $Title
@onready var body_label: Label3D = $Body

var _material: StandardMaterial3D


func _ready() -> void:
	add_to_group("spatial_panels")
	title_label.text = title_text
	body_label.text = body_text
	_material = StandardMaterial3D.new()
	_material.albedo_color = Color("171827")
	_material.metallic = 0.15
	_material.roughness = 0.72
	_material.emission_enabled = true
	_material.emission = accent_color
	_material.emission_energy_multiplier = 0.12
	surface.material = _material


func set_focused(value: bool) -> void:
	_material.emission_energy_multiplier = 0.65 if value else 0.12
	title_label.modulate = accent_color if value else Color.WHITE
