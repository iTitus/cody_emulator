;
; codymelody.asm
; Simple multi-voice SID-style melody loop for the Cody Computer.
; Plays once on start, then replays when SPACE is pressed.
;
; Assemble with 64tass:
;   64tass --mw65c02 --nostart -o codymelody.bin codymelody.asm
;

ADDR      = $0300               ; Load address

SID_BASE  = $D400
SID_V1FL  = SID_BASE+0
SID_V1FH  = SID_BASE+1
SID_V1PL  = SID_BASE+2
SID_V1PH  = SID_BASE+3
SID_V1CT  = SID_BASE+4
SID_V1AD  = SID_BASE+5
SID_V1SR  = SID_BASE+6

SID_V2FL  = SID_BASE+7
SID_V2FH  = SID_BASE+8
SID_V2PL  = SID_BASE+9
SID_V2PH  = SID_BASE+10
SID_V2CT  = SID_BASE+11
SID_V2AD  = SID_BASE+12
SID_V2SR  = SID_BASE+13

SID_V3FL  = SID_BASE+14
SID_V3FH  = SID_BASE+15
SID_V3PL  = SID_BASE+16
SID_V3PH  = SID_BASE+17
SID_V3CT  = SID_BASE+18
SID_V3AD  = SID_BASE+19
SID_V3SR  = SID_BASE+20

SID_FVOL  = SID_BASE+24

VIA_BASE  = $9F00
VIA_IORB  = VIA_BASE+$0
VIA_IORA  = VIA_BASE+$1
VIA_DDRB  = VIA_BASE+$2
VIA_DDRA  = VIA_BASE+$3

KEYROW0   = $DA                 ; Keyboard row 0
KEYROW1   = $DB
KEYROW2   = $DC
KEYROW3   = $DD
KEYROW4   = $DE
KEYROW5   = $DF
KEYMODS   = $E0
KEYCODE   = $E1

KEY_CODY  = $0B
KEY_META  = $0F
KEY_SPACE = $19

CTRL_GATE = $01
CTRL_TRI  = $10
CTRL_SAW  = $20
CTRL_PULSE= $40

; Pitch constants derived from: reg = round(hz * 262144 / 16000)
NOTE_C3   = $0860  ; 130.81 Hz
NOTE_D3   = $0966  ; 146.83 Hz
NOTE_E3   = $0A8D  ; 164.81 Hz
NOTE_F3   = $0B2D  ; 174.61 Hz
NOTE_G3   = $0C8B  ; 196.00 Hz
NOTE_A3   = $0E14  ; 220.00 Hz

NOTE_C4   = $10BF  ; 261.63 Hz
NOTE_D4   = $12FE  ; 293.66 Hz
NOTE_E4   = $1513  ; 329.63 Hz
NOTE_F4   = $1675  ; 349.23 Hz
NOTE_G4   = $1917  ; 392.00 Hz
NOTE_A4   = $1C29  ; 440.00 Hz
NOTE_B4   = $1F9F  ; 493.88 Hz
NOTE_C5   = $217F  ; 523.25 Hz
NOTE_D5   = $2596  ; 587.33 Hz
NOTE_REST = $0000

SONG_LEN  = 20

* = ADDR

MAIN        SEI
            JSR INIT_AUDIO
            CLI

            JSR PLAY_SONG       ; Play once at startup

WAIT_SPACE  JSR KEYSCAN
            JSR KEYDECODE
            LDA KEYCODE
            AND #$1F
            CMP #KEY_SPACE
            BNE WAIT_SPACE

            JSR WAIT_RELEASE
            JSR PLAY_SONG
            BRA WAIT_SPACE

;
; INIT_AUDIO
; Initialize SID registers, ADSR, and pulse width.
;
INIT_AUDIO  LDA #$0F            ; Max volume, voice 3 enabled
            STA SID_FVOL

            LDA #$24            ; V1: softer lead
            STA SID_V1AD
            LDA #$46            ; V2: plucky pulse
            STA SID_V2AD
            LDA #$13            ; V3: slower bass
            STA SID_V3AD

            LDA #$75            ; V1: sustain 7, release 5
            STA SID_V1SR
            LDA #$53            ; V2: sustain 5, release 3
            STA SID_V2SR
            LDA #$A6            ; V3: sustain 10, release 6
            STA SID_V3SR

            LDA #$00            ; Pulse width (approx 50%) for voice 2
            STA SID_V2PL
            LDA #$08
            STA SID_V2PH

            LDA #$07            ; VIA: bits 0-2 output, 3-7 input
            STA VIA_DDRA

            LDA #CTRL_TRI       ; Default waveforms, gate off
            STA SID_V1CT
            LDA #CTRL_PULSE
            STA SID_V2CT
            LDA #CTRL_SAW
            STA SID_V3CT

            RTS

;
; PLAY_SONG
; Plays a short loop (~5s) with 3 voices.
;
PLAY_SONG   LDX #0

_PLAY       CPX #SONG_LEN
            BEQ _DONE

            LDA MELODY_LO,X
            STA SID_V1FL
            LDA MELODY_HI,X
            STA SID_V1FH

            LDA HARM_LO,X
            STA SID_V2FL
            LDA HARM_HI,X
            STA SID_V2FH

            LDA BASS_LO,X
            STA SID_V3FL
            LDA BASS_HI,X
            STA SID_V3FH

            LDA V1_CTRL,X
            JSR APPLY_GATE_V1
            LDA V2_CTRL,X
            JSR APPLY_GATE_V2
            LDA V3_CTRL,X
            JSR APPLY_GATE_V3

            LDA DURATIONS,X
            JSR DELAY_TICKS

            LDA V1_CTRL,X
            STA SID_V1CT
            LDA V2_CTRL,X
            STA SID_V2CT
            LDA V3_CTRL,X
            STA SID_V3CT

            LDA #1              ; Short gap between notes
            JSR DELAY_TICKS

            INX
            BRA _PLAY

_DONE       RTS

;
; WAIT_RELEASE
; Wait until SPACE is released.
;
WAIT_RELEASE
            JSR KEYSCAN
            JSR KEYDECODE
            LDA KEYCODE
            AND #$1F
            CMP #KEY_SPACE
            BEQ WAIT_RELEASE
            RTS

;
; KEYSCAN
; Scan the keyboard matrix into KEYROWx.
;
KEYSCAN     PHA
            PHX

            STZ VIA_IORA
            LDX #0

_SCAN       LDA VIA_IORA
            LSR A
            LSR A
            LSR A
            STA KEYROW0,X

            INC VIA_IORA
            INX

            CPX #6
            BNE _SCAN

            PLX
            PLA
            RTS

;
; KEYDECODE
; Decode KEYROWx into KEYCODE/KEYMODS (mirrors Cody BASIC scan logic).
;
KEYDECODE  PHX
            PHY

            STZ KEYMODS
            STZ KEYCODE

            LDX #0
            LDY #0

_ROW        LDA KEYROW0,X
            INX

            PHX
            LDX #5

_COL        INY
            LSR A
            BCS _NEXT

            CPY #KEY_META
            BNE _CODY
            PHA
            LDA KEYMODS
            ORA #$20
            STA KEYMODS
            PLA
            BRA _NEXT

_CODY       CPY #KEY_CODY
            BNE _NORM
            PHA
            LDA KEYMODS
            ORA #$40
            STA KEYMODS
            PLA
            BRA _NEXT

_NORM       PHA
            TYA
            STA KEYCODE
            PLA

_NEXT       DEX
            BNE _COL

            PLX
            CPX #6
            BNE _ROW

            LDA KEYCODE
            ORA KEYMODS
            STA KEYCODE

            PLY
            PLX
            RTS

;
; DELAY_TICKS
; Busy-wait delay: A = number of ticks.
;
DELAY_TICKS PHX
            TAX
_TLOOP      JSR DELAY_UNIT
            DEX
            BNE _TLOOP
            PLX
            RTS

;
; DELAY_UNIT
; Inner delay loop (tune here for tempo).
;
DELAY_UNIT  PHX
            PHY
            LDX #$30
_OUTER      LDY #$FF
_INNER      DEY
            BNE _INNER
            DEX
            BNE _OUTER
            PLY
            PLX
            RTS

;
; APPLY_GATE_V1/V2/V3
; Adds gate if the corresponding note is not REST.
;
APPLY_GATE_V1
            PHA
            LDA MELODY_LO,X
            ORA MELODY_HI,X
            BEQ _V1_OFF
            PLA
            ORA #CTRL_GATE
            STA SID_V1CT
            RTS
_V1_OFF     PLA
            STA SID_V1CT
            RTS

APPLY_GATE_V2
            PHA
            LDA HARM_LO,X
            ORA HARM_HI,X
            BEQ _V2_OFF
            PLA
            ORA #CTRL_GATE
            STA SID_V2CT
            RTS
_V2_OFF     PLA
            STA SID_V2CT
            RTS

APPLY_GATE_V3
            PHA
            LDA BASS_LO,X
            ORA BASS_HI,X
            BEQ _V3_OFF
            PLA
            ORA #CTRL_GATE
            STA SID_V3CT
            RTS
_V3_OFF     PLA
            STA SID_V3CT
            RTS

; Melody tables (original, loosely inspired by classic 8-bit overworld motifs)

MELODY_LO  .BYTE <NOTE_C4, <NOTE_E4, <NOTE_G4, <NOTE_E4
           .BYTE <NOTE_D4, <NOTE_F4, <NOTE_A4, <NOTE_F4
           .BYTE <NOTE_E4, <NOTE_G4, <NOTE_C5, <NOTE_G4
           .BYTE <NOTE_D4, <NOTE_F4, <NOTE_B4, <NOTE_F4
           .BYTE <NOTE_C4, <NOTE_REST, <NOTE_C4, <NOTE_G4

MELODY_HI  .BYTE >NOTE_C4, >NOTE_E4, >NOTE_G4, >NOTE_E4
           .BYTE >NOTE_D4, >NOTE_F4, >NOTE_A4, >NOTE_F4
           .BYTE >NOTE_E4, >NOTE_G4, >NOTE_C5, >NOTE_G4
           .BYTE >NOTE_D4, >NOTE_F4, >NOTE_B4, >NOTE_F4
           .BYTE >NOTE_C4, >NOTE_REST, >NOTE_C4, >NOTE_G4

HARM_LO    .BYTE <NOTE_E4, <NOTE_G4, <NOTE_B4, <NOTE_G4
           .BYTE <NOTE_F4, <NOTE_A4, <NOTE_C5, <NOTE_A4
           .BYTE <NOTE_G4, <NOTE_B4, <NOTE_D5, <NOTE_B4
           .BYTE <NOTE_F4, <NOTE_A4, <NOTE_C5, <NOTE_A4
           .BYTE <NOTE_E4, <NOTE_REST, <NOTE_E4, <NOTE_D4

HARM_HI    .BYTE >NOTE_E4, >NOTE_G4, >NOTE_B4, >NOTE_G4
           .BYTE >NOTE_F4, >NOTE_A4, >NOTE_C5, >NOTE_A4
           .BYTE >NOTE_G4, >NOTE_B4, >NOTE_D5, >NOTE_B4
           .BYTE >NOTE_F4, >NOTE_A4, >NOTE_C5, >NOTE_A4
           .BYTE >NOTE_E4, >NOTE_REST, >NOTE_E4, >NOTE_D4

BASS_LO    .BYTE <NOTE_C3, <NOTE_C3, <NOTE_C3, <NOTE_C3
           .BYTE <NOTE_C3, <NOTE_C3, <NOTE_C3, <NOTE_C3
           .BYTE <NOTE_G3, <NOTE_G3, <NOTE_G3, <NOTE_G3
           .BYTE <NOTE_C3, <NOTE_C3, <NOTE_C4, <NOTE_C3
           .BYTE <NOTE_C3, <NOTE_REST, <NOTE_C3, <NOTE_G3

BASS_HI    .BYTE >NOTE_C3, >NOTE_C3, >NOTE_C3, >NOTE_C3
           .BYTE >NOTE_C3, >NOTE_C3, >NOTE_C3, >NOTE_C3
           .BYTE >NOTE_G3, >NOTE_G3, >NOTE_G3, >NOTE_G3
           .BYTE >NOTE_C3, >NOTE_C3, >NOTE_C4, >NOTE_C3
           .BYTE >NOTE_C3, >NOTE_REST, >NOTE_C3, >NOTE_G3

DURATIONS  .BYTE 4,4,4,4, 4,4,4,4, 4,4,4,4, 4,4,4,4, 6,2,4,6

V1_CTRL    .BYTE CTRL_TRI,CTRL_TRI,CTRL_TRI,CTRL_TRI
           .BYTE CTRL_TRI,CTRL_TRI,CTRL_TRI,CTRL_TRI
           .BYTE CTRL_TRI,CTRL_TRI,CTRL_TRI,CTRL_TRI
           .BYTE CTRL_TRI,CTRL_TRI,CTRL_TRI,CTRL_TRI
           .BYTE CTRL_TRI,CTRL_TRI,CTRL_PULSE,CTRL_PULSE

V2_CTRL    .BYTE CTRL_PULSE,CTRL_PULSE,CTRL_PULSE,CTRL_PULSE
           .BYTE CTRL_PULSE,CTRL_PULSE,CTRL_PULSE,CTRL_PULSE
           .BYTE CTRL_PULSE,CTRL_PULSE,CTRL_PULSE,CTRL_PULSE
           .BYTE CTRL_PULSE,CTRL_PULSE,CTRL_PULSE,CTRL_PULSE
           .BYTE CTRL_PULSE,CTRL_PULSE,CTRL_PULSE,CTRL_PULSE

V3_CTRL    .BYTE CTRL_SAW,CTRL_SAW,CTRL_SAW,CTRL_SAW
           .BYTE CTRL_SAW,CTRL_SAW,CTRL_SAW,CTRL_SAW
           .BYTE CTRL_SAW,CTRL_SAW,CTRL_SAW,CTRL_SAW
           .BYTE CTRL_SAW,CTRL_SAW,CTRL_SAW,CTRL_SAW
           .BYTE CTRL_SAW,CTRL_SAW,CTRL_SAW,CTRL_SAW

LAST
