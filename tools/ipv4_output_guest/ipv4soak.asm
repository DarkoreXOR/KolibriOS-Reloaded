;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; IPV4SOAK — disposable MENUET01 UDP emitter for ipv4_output evidence.
;; Not a production app. Assembled by scripts/qmp_ipv4_output_soak.py.
;;
;; Intended as firstapp (/sys/IPV4SOAK, same length as /sys/LAUNCHER):
;;   START marker, wait for NIC (or spawn LAUNCHER then wait), set
;;   10.0.2.15/24 gw 10.0.2.2, UDP send IPV4SOAK{n} to 10.0.2.2:9.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

use32
org 0

db 'MENUET01'
dd 1
dd start
dd i_end
dd mem
dd stack_top
dd 0
dd 0

AF_INET4        = 2
SOCK_DGRAM      = 2
IP_10_0_2_15    = 0x0F02000A
IP_10_0_2_2     = 0x0202000A
MASK_24         = 0x00FFFFFF
PORT9_NET       = 0x0900                ; htons(9)
EBX_DEVCOUNT    = 0xFF
EBX_READ_IP     = 0x00010102            ; IPv4, device 1, read IP
EBX_WRITE_IP    = 0x00010103
EBX_WRITE_MASK  = 0x00010107
EBX_WRITE_GW    = 0x00010109

start:
        mov     esi, msg_start
        call    board_puts

        call    wait_nic
        jnc     .have_nic

        mov     esi, msg_launch
        call    board_puts
        mov     eax, 70
        mov     ebx, file70_launcher
        int     0x40
        call    wait_nic
        jnc     .have_nic
        mov     esi, msg_fail_nic
        call    board_puts
        jmp     .exit

.have_nic:
        mov     esi, msg_nic
        call    board_puts

        mov     eax, 76
        mov     ebx, EBX_WRITE_IP
        mov     ecx, IP_10_0_2_15
        int     0x40
        mov     eax, 76
        mov     ebx, EBX_WRITE_MASK
        mov     ecx, MASK_24
        int     0x40
        mov     eax, 76
        mov     ebx, EBX_WRITE_GW
        mov     ecx, IP_10_0_2_2
        int     0x40

        mov     eax, 5
        mov     ebx, 50                 ; 0.50 s for gratuitous ARP
        int     0x40

        mov     eax, 76
        mov     ebx, EBX_READ_IP
        int     0x40
        cmp     eax, -1
        je      .fail_ip
        test    eax, eax
        jz      .fail_ip
        mov     esi, msg_ip
        call    board_puts

        mov     eax, 75
        xor     ebx, ebx
        mov     ecx, AF_INET4
        mov     edx, SOCK_DGRAM
        xor     esi, esi
        int     0x40
        cmp     eax, -1
        je      .fail_sock
        mov     [sock], eax

        mov     eax, 75
        mov     ebx, 4
        mov     ecx, [sock]
        mov     edx, sockaddr_dst
        mov     esi, 16
        int     0x40
        cmp     eax, -1
        je      .fail_conn

.send_loop:
        mov     al, [send_count]
        add     al, '0'
        mov     [payload_seq], al
        mov     eax, 75
        mov     ebx, 6
        mov     ecx, [sock]
        mov     edx, payload
        mov     esi, payload_len
        xor     edi, edi
        int     0x40
        cmp     eax, -1
        je      .send_retry
        mov     esi, msg_send
        call    board_puts
        inc     byte [ok_sends]
.send_retry:
        mov     eax, 5
        mov     ebx, 50
        int     0x40
        inc     byte [send_count]
        cmp     byte [send_count], 5
        jb      .send_loop

        mov     eax, 75
        mov     ebx, 1
        mov     ecx, [sock]
        int     0x40

        cmp     byte [ok_sends], 0
        je      .fail_send
        mov     esi, msg_pass
        call    board_puts
        jmp     .exit

.fail_ip:
        mov     esi, msg_fail_ip
        call    board_puts
        jmp     .exit
.fail_sock:
        mov     esi, msg_fail_sock
        call    board_puts
        jmp     .exit
.fail_conn:
        mov     esi, msg_fail_conn
        call    board_puts
        jmp     .exit
.fail_send:
        mov     esi, msg_fail_send
        call    board_puts

.exit:
        mov     eax, 5
        mov     ebx, 100
        int     0x40
        or      eax, -1
        int     0x40

; CF=1 if NIC never reached count>=2
wait_nic:
        mov     ecx, 80                 ; 80 * 0.25 s = 20 s
.wn:
        push    ecx
        mov     eax, 74
        mov     ebx, EBX_DEVCOUNT
        int     0x40
        cmp     eax, 2
        jae     .wn_ok
        mov     eax, 5
        mov     ebx, 25
        int     0x40
        pop     ecx
        loop    .wn
        stc
        ret
.wn_ok:
        pop     ecx
        clc
        ret

board_puts:
        lodsb
        test    al, al
        jz      .done
        push    esi
        mov     ebx, 1
        movzx   ecx, al
        mov     eax, 63
        int     0x40
        pop     esi
        jmp     board_puts
.done:
        ret

align 4
sock            dd 0
send_count      db 0
ok_sends        db 0

align 4
file70_launcher:
        dd      7, 0, 0, 0, 0
        db      '/sys/LAUNCHER',0

align 4
sockaddr_dst:
        dw      AF_INET4
        dw      PORT9_NET
        dd      IP_10_0_2_2
        dq      0

payload:
        db      'IPV4SOAK'
payload_seq:
        db      '0'
        db      10
payload_len = $ - payload

msg_start       db 'IPV4SOAK START',13,10,0
msg_launch      db 'IPV4SOAK LAUNCH',13,10,0
msg_nic         db 'IPV4SOAK NIC',13,10,0
msg_ip          db 'IPV4SOAK IP',13,10,0
msg_send        db 'IPV4SOAK SEND',13,10,0
msg_pass        db 'IPV4SOAK PASS',13,10,0
msg_fail_nic    db 'IPV4SOAK FAIL NIC',13,10,0
msg_fail_ip     db 'IPV4SOAK FAIL IP',13,10,0
msg_fail_sock   db 'IPV4SOAK FAIL SOCK',13,10,0
msg_fail_conn   db 'IPV4SOAK FAIL CONN',13,10,0
msg_fail_send   db 'IPV4SOAK FAIL SEND',13,10,0

i_end:
align 16
        rb      0x400
stack_top:
mem:
