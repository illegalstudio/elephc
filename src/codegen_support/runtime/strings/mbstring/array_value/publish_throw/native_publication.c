/* Independent ownership checks for the emitted boxed-Throwable publication action. */
#include <assert.h>
#include <stdint.h>
#include <stddef.h>

struct object { unsigned owners; };
struct cell { uint64_t tag, lo, hi; };
__attribute__((visibility("hidden"))) void *pending __asm__("_exc_value");
extern int publish(void *, struct cell *) __asm__("__rt_mbstring_publish_throw");
int release_owner(void *, struct cell *) __asm__("__rt_mbstring_release");
static unsigned retained, released;
static int release_status;

/* Records the raw-object owner acquired by real publication assembly. */
void mb_test_incref(struct object *object) {
    assert(object != NULL);
    retained++;
    object->owners++;
}

/* Consumes input ownership and verifies publication preceded the release callback. */
int release_owner(void *context, struct cell *cell) {
    assert(context == NULL);
    released++;
    if (cell != NULL && cell->tag == 6) {
        struct object *object = (struct object *)(uintptr_t)cell->lo;
        assert(pending == object);
        assert(object->owners == 2);
        object->owners--;
    } else {
        assert(pending == NULL);
    }
    return release_status;
}

/* Exercises successful transfer, callback failures, malformed tags, and absent inputs. */
int main(void) {
    for (release_status = 0; release_status <= 2; release_status++) {
        struct object object = {1};
        struct cell box = {6, (uintptr_t)&object, 0};
        retained = released = 0;
        pending = NULL;
        assert(publish(NULL, &box) == (release_status == 0 ? 0 : 2));
        assert(pending == &object);
        assert(object.owners == 1 && retained == 1 && released == 1);
    }
    for (release_status = 0; release_status <= 2; release_status++) {
        struct cell invalid = {1, (uintptr_t)"invalid", 7};
        retained = released = 0;
        pending = NULL;
        assert(publish(NULL, &invalid) == (release_status == 2 ? 2 : 1));
        assert(retained == 0 && released == 1);
    }
    release_status = 0;
    retained = released = 0;
    pending = NULL;
    assert(publish(NULL, NULL) == 1);
    assert(retained == 0 && released == 1);
    return 0;
}
