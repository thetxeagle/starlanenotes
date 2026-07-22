#include "bindings/bindings.h"
#import "BackgroundReplayScheduler.h"

int main(int argc, char * argv[]) {
	[StarlaneBackgroundReplayScheduler registerDefaultSchedules];
	ffi::start_app();
	return 0;
}
