#ifndef PSY_BRIDGE_H
#define PSY_BRIDGE_H

#include <stdint.h>

typedef struct PsySaver PsySaver;

PsySaver *psy_create(void *layer, uint64_t seed);
void psy_destroy(PsySaver *saver);
void psy_resize(PsySaver *saver, double width, double height);
void psy_frame(PsySaver *saver, float delta_seconds);
void psy_set_scene_seconds(PsySaver *saver, float seconds);
void psy_set_speed(PsySaver *saver, float speed);
void psy_set_mutation_strength(PsySaver *saver, float strength);
uint64_t psy_frames_presented(PsySaver *saver);
void psy_advance_scene(PsySaver *saver);

#endif
