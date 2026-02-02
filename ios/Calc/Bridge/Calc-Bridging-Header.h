//
//  Calc-Bridging-Header.h
//  Calc
//
//  Use this file to import C headers into Swift
//

#ifndef Calc_Bridging_Header_h
#define Calc_Bridging_Header_h

// Use emu_backend.h for dual-backend support (provides same API as emu.h)
// Falls back to single-backend behavior when only one backend is linked
#import "emu_backend.h"

#endif /* Calc_Bridging_Header_h */
