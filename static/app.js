// Axi-Vid WebRTC Video Chat Client
// Handles WebSocket signaling and peer-to-peer video/audio connections

(function() {
    'use strict';

    // Configuration
    const CONFIG = {
        iceServers: [
            { urls: 'stun:stun.l.google.com:19302' },
            { urls: 'stun:stun1.l.google.com:19302' }
        ],
        mediaConstraints: {
            video: {
                width: { ideal: 1280 },
                height: { ideal: 720 },
                facingMode: 'user'
            },
            audio: true
        },
        reconnectAttempts: 5,
        reconnectDelay: 1000
    };

    // State
    let ws = null;
    let peerConnection = null;
    let localStream = null;
    let remoteStream = null;
    let isAudioEnabled = true;
    let isVideoEnabled = true;
    let reconnectAttempts = 0;
    let isCallActive = false;
    let isCaller = false;

    // DOM Elements
    const elements = {
        roomIdDisplay: document.getElementById('room-id-display'),
        copyLinkBtn: document.getElementById('copy-link-btn'),
        connectionStatus: document.getElementById('connection-status'),
        localVideo: document.getElementById('local-video'),
        remoteVideo: document.getElementById('remote-video'),
        remoteStatus: document.getElementById('remote-status'),
        startCallBtn: document.getElementById('start-call-btn'),
        toggleAudioBtn: document.getElementById('toggle-audio-btn'),
        toggleVideoBtn: document.getElementById('toggle-video-btn'),
        hangUpBtn: document.getElementById('hang-up-btn'),
        chatMessages: document.getElementById('chat-messages'),
        chatInput: document.getElementById('chat-input'),
        sendChatBtn: document.getElementById('send-chat-btn'),
        toggleChatBtn: document.getElementById('toggle-chat-btn'),
        chatContainer: document.getElementById('chat-container'),
        waitingOverlay: document.getElementById('waiting-overlay'),
        permissionOverlay: document.getElementById('permission-overlay'),
        permissionErrorMsg: document.getElementById('permission-error-msg'),
        retryPermissionBtn: document.getElementById('retry-permission-btn')
    };

    // Initialize
    function init() {
        const roomId = window.ROOM_ID;
        if (!roomId || roomId === '{{ROOM_ID}}') {
            setStatus('Error: Invalid room', 'error');
            return;
        }

        elements.roomIdDisplay.textContent = `Room: ${roomId.substring(0, 8)}...`;
        setupEventListeners();
        connectWebSocket(roomId);
    }

    // Setup event listeners
    function setupEventListeners() {
        elements.copyLinkBtn.addEventListener('click', copyRoomLink);
        elements.startCallBtn.addEventListener('click', startCall);
        elements.toggleAudioBtn.addEventListener('click', toggleAudio);
        elements.toggleVideoBtn.addEventListener('click', toggleVideo);
        elements.hangUpBtn.addEventListener('click', hangUp);
        elements.sendChatBtn.addEventListener('click', sendChatMessage);
        elements.chatInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') sendChatMessage();
        });
        elements.toggleChatBtn.addEventListener('click', toggleChat);
        elements.retryPermissionBtn.addEventListener('click', () => {
            elements.permissionOverlay.classList.add('hidden');
            startCall();
        });
    }

    // WebSocket connection
    function connectWebSocket(roomId) {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws/${roomId}`;

        setStatus('Connecting...', 'connecting');
        ws = new WebSocket(wsUrl);

        ws.onopen = () => {
            console.log('WebSocket connected');
            setStatus('Connected - waiting for peer', 'waiting');
            reconnectAttempts = 0;
            enableChat(true);
        };

        ws.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                handleSignalingMessage(msg);
            } catch (e) {
                console.error('Failed to parse message:', e);
            }
        };

        ws.onclose = () => {
            console.log('WebSocket closed');
            setStatus('Disconnected', 'disconnected');
            enableChat(false);

            if (reconnectAttempts < CONFIG.reconnectAttempts) {
                reconnectAttempts++;
                const delay = CONFIG.reconnectDelay * Math.pow(2, reconnectAttempts - 1);
                console.log(`Reconnecting in ${delay}ms (attempt ${reconnectAttempts})`);
                setTimeout(() => connectWebSocket(roomId), delay);
            }
        };

        ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            setStatus('Connection error', 'error');
        };
    }

    // Handle incoming signaling messages
    function handleSignalingMessage(msg) {
        console.log('Received:', msg.type);

        switch (msg.type) {
            case 'room_info':
                handleRoomInfo(msg);
                break;
            case 'join':
                handlePeerJoined();
                break;
            case 'leave':
                handlePeerLeft();
                break;
            case 'offer':
                handleOffer(msg);
                break;
            case 'answer':
                handleAnswer(msg);
                break;
            case 'ice':
                handleIceCandidate(msg);
                break;
            case 'chat':
                handleChatMessage(msg);
                break;
            case 'media_status':
                handleMediaStatus(msg);
                break;
            case 'error':
                handleError(msg);
                break;
        }
    }

    function handleRoomInfo(msg) {
        const peerCount = msg.peer_count;
        if (peerCount === 1) {
            setStatus('Waiting for peer...', 'waiting');
            elements.waitingOverlay.classList.remove('hidden');
        } else if (peerCount === 2) {
            setStatus('Peer connected', 'connected');
            elements.waitingOverlay.classList.add('hidden');
        }
    }

    function handlePeerJoined() {
        setStatus('Peer joined', 'connected');
        elements.waitingOverlay.classList.add('hidden');
        addSystemMessage('A peer has joined the room');

        // If we're already in a call and have local stream, we're the caller
        if (localStream && !peerConnection) {
            isCaller = true;
            createPeerConnection();
            createOffer();
        }
    }

    function handlePeerLeft() {
        setStatus('Peer left', 'waiting');
        addSystemMessage('Peer has left the room');
        elements.remoteStatus.textContent = '';

        if (peerConnection) {
            peerConnection.close();
            peerConnection = null;
        }

        elements.remoteVideo.srcObject = null;
        remoteStream = null;
        isCallActive = false;
        updateControlButtons();
    }

    async function handleOffer(msg) {
        console.log('Handling offer');
        if (!localStream) {
            await getLocalStream();
        }

        isCaller = false;
        createPeerConnection();

        try {
            await peerConnection.setRemoteDescription(new RTCSessionDescription({
                type: 'offer',
                sdp: msg.sdp
            }));

            const answer = await peerConnection.createAnswer();
            await peerConnection.setLocalDescription(answer);

            sendMessage({
                type: 'answer',
                sdp: answer.sdp
            });

            isCallActive = true;
            updateControlButtons();
        } catch (e) {
            console.error('Error handling offer:', e);
        }
    }

    async function handleAnswer(msg) {
        console.log('Handling answer');
        try {
            await peerConnection.setRemoteDescription(new RTCSessionDescription({
                type: 'answer',
                sdp: msg.sdp
            }));
        } catch (e) {
            console.error('Error handling answer:', e);
        }
    }

    async function handleIceCandidate(msg) {
        if (!peerConnection) return;

        try {
            await peerConnection.addIceCandidate(new RTCIceCandidate({
                candidate: msg.candidate,
                sdpMLineIndex: msg.sdpMLineIndex,
                sdpMid: msg.sdpMid
            }));
        } catch (e) {
            console.error('Error adding ICE candidate:', e);
        }
    }

    function handleChatMessage(msg) {
        addChatMessage(msg.message, false);
    }

    function handleMediaStatus(msg) {
        const status = [];
        if (!msg.audio) status.push('muted');
        if (!msg.video) status.push('video off');
        elements.remoteStatus.textContent = status.length ? status.join(', ') : '';
    }

    function handleError(msg) {
        console.error('Server error:', msg.message);
        setStatus(`Error: ${msg.message}`, 'error');
        addSystemMessage(`Error: ${msg.message}`);
    }

    // Get local media stream
    async function getLocalStream() {
        try {
            localStream = await navigator.mediaDevices.getUserMedia(CONFIG.mediaConstraints);
            elements.localVideo.srcObject = localStream;
            return true;
        } catch (e) {
            console.error('Error getting media:', e);
            elements.permissionErrorMsg.textContent = getMediaErrorMessage(e);
            elements.permissionOverlay.classList.remove('hidden');
            return false;
        }
    }

    function getMediaErrorMessage(error) {
        switch (error.name) {
            case 'NotAllowedError':
                return 'Camera/microphone access was denied. Please allow access in your browser settings.';
            case 'NotFoundError':
                return 'No camera or microphone found. Please connect a device and try again.';
            case 'NotReadableError':
                return 'Camera or microphone is already in use by another application.';
            default:
                return `Failed to access media: ${error.message}`;
        }
    }

    // Create peer connection
    function createPeerConnection() {
        if (peerConnection) {
            peerConnection.close();
        }

        peerConnection = new RTCPeerConnection({ iceServers: CONFIG.iceServers });

        // Add local tracks
        if (localStream) {
            localStream.getTracks().forEach(track => {
                peerConnection.addTrack(track, localStream);
            });
        }

        // Handle ICE candidates
        peerConnection.onicecandidate = (event) => {
            if (event.candidate) {
                sendMessage({
                    type: 'ice',
                    candidate: event.candidate.candidate,
                    sdpMLineIndex: event.candidate.sdpMLineIndex,
                    sdpMid: event.candidate.sdpMid
                });
            }
        };

        // Handle remote tracks
        peerConnection.ontrack = (event) => {
            console.log('Remote track received');
            if (!remoteStream) {
                remoteStream = new MediaStream();
                elements.remoteVideo.srcObject = remoteStream;
            }
            remoteStream.addTrack(event.track);
        };

        // Handle connection state changes
        peerConnection.onconnectionstatechange = () => {
            console.log('Connection state:', peerConnection.connectionState);
            switch (peerConnection.connectionState) {
                case 'connected':
                    setStatus('Call connected', 'connected');
                    break;
                case 'disconnected':
                    setStatus('Call disconnected', 'disconnected');
                    break;
                case 'failed':
                    setStatus('Connection failed', 'error');
                    handleConnectionFailure();
                    break;
            }
        };

        // Handle ICE connection state
        peerConnection.oniceconnectionstatechange = () => {
            console.log('ICE state:', peerConnection.iceConnectionState);
            if (peerConnection.iceConnectionState === 'failed') {
                handleConnectionFailure();
            }
        };
    }

    // Create and send offer
    async function createOffer() {
        try {
            const offer = await peerConnection.createOffer();
            await peerConnection.setLocalDescription(offer);

            sendMessage({
                type: 'offer',
                sdp: offer.sdp
            });

            isCallActive = true;
            updateControlButtons();
        } catch (e) {
            console.error('Error creating offer:', e);
        }
    }

    // Handle connection failure
    function handleConnectionFailure() {
        if (isCallActive && peerConnection) {
            console.log('Attempting to reconnect...');
            // Trigger ICE restart
            peerConnection.restartIce();
            if (isCaller) {
                createOffer();
            }
        }
    }

    // Start call
    async function startCall() {
        if (!await getLocalStream()) return;

        elements.startCallBtn.disabled = true;
        isCaller = true;
        createPeerConnection();

        // If peer is already in room, create offer
        // Otherwise wait for peer_joined event
        const response = await fetch(`/api/room/${window.ROOM_ID}/status`);
        const status = await response.json();

        if (status.peer_count === 2) {
            createOffer();
        } else {
            setStatus('Waiting for peer...', 'waiting');
            elements.waitingOverlay.classList.remove('hidden');
        }

        updateControlButtons();
    }

    // Toggle audio
    function toggleAudio() {
        if (!localStream) return;

        isAudioEnabled = !isAudioEnabled;
        localStream.getAudioTracks().forEach(track => {
            track.enabled = isAudioEnabled;
        });

        elements.toggleAudioBtn.textContent = isAudioEnabled ? 'Mute' : 'Unmute';
        sendMediaStatus();
    }

    // Toggle video
    function toggleVideo() {
        if (!localStream) return;

        isVideoEnabled = !isVideoEnabled;
        localStream.getVideoTracks().forEach(track => {
            track.enabled = isVideoEnabled;
        });

        elements.toggleVideoBtn.textContent = isVideoEnabled ? 'Hide Video' : 'Show Video';
        sendMediaStatus();
    }

    // Send media status to peer
    function sendMediaStatus() {
        sendMessage({
            type: 'media_status',
            audio: isAudioEnabled,
            video: isVideoEnabled
        });
    }

    // Hang up
    function hangUp() {
        if (peerConnection) {
            peerConnection.close();
            peerConnection = null;
        }

        if (localStream) {
            localStream.getTracks().forEach(track => track.stop());
            localStream = null;
        }

        elements.localVideo.srcObject = null;
        elements.remoteVideo.srcObject = null;
        remoteStream = null;
        isCallActive = false;
        isCaller = false;

        sendMessage({ type: 'leave' });
        updateControlButtons();
        setStatus('Call ended', 'disconnected');
    }

    // Update control button states
    function updateControlButtons() {
        const hasLocalStream = !!localStream;
        elements.startCallBtn.disabled = hasLocalStream;
        elements.toggleAudioBtn.disabled = !hasLocalStream;
        elements.toggleVideoBtn.disabled = !hasLocalStream;
        elements.hangUpBtn.disabled = !hasLocalStream;
    }

    // Chat functions
    function enableChat(enabled) {
        elements.chatInput.disabled = !enabled;
        elements.sendChatBtn.disabled = !enabled;
    }

    function sendChatMessage() {
        const message = elements.chatInput.value.trim();
        if (!message) return;

        sendMessage({
            type: 'chat',
            message: message
        });

        addChatMessage(message, true);
        elements.chatInput.value = '';
    }

    function addChatMessage(message, isOwn) {
        const div = document.createElement('div');
        div.className = `chat-message ${isOwn ? 'own' : 'peer'}`;
        div.textContent = message;
        elements.chatMessages.appendChild(div);
        elements.chatMessages.scrollTop = elements.chatMessages.scrollHeight;
    }

    function addSystemMessage(message) {
        const div = document.createElement('div');
        div.className = 'chat-message system';
        div.textContent = message;
        elements.chatMessages.appendChild(div);
        elements.chatMessages.scrollTop = elements.chatMessages.scrollHeight;
    }

    function toggleChat() {
        elements.chatContainer.classList.toggle('hidden');
        elements.toggleChatBtn.textContent =
            elements.chatContainer.classList.contains('hidden') ? 'Show' : 'Hide';
    }

    // Utility functions
    function sendMessage(msg) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(msg));
        }
    }

    function setStatus(text, type) {
        elements.connectionStatus.textContent = text;
        elements.connectionStatus.className = `status ${type}`;
    }

    function copyRoomLink() {
        const url = window.location.href;
        navigator.clipboard.writeText(url).then(() => {
            const originalText = elements.copyLinkBtn.textContent;
            elements.copyLinkBtn.textContent = 'Copied!';
            setTimeout(() => {
                elements.copyLinkBtn.textContent = originalText;
            }, 2000);
        });
    }

    // Start the app
    document.addEventListener('DOMContentLoaded', init);
})();
