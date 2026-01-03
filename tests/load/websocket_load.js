// K6 WebSocket Load Test for Ara Chat Service
// Target: 10M concurrent connections simulation
//
// Usage:
//   k6 run --vus 1000 --duration 5m websocket_load.js
//   k6 run --vus 10000 --duration 30m --env TARGET=ws://chat:8082 websocket_load.js

import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { randomString, randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Custom metrics
const wsConnections = new Gauge('ws_active_connections');
const wsMessages = new Counter('ws_messages_sent');
const wsMessagesReceived = new Counter('ws_messages_received');
const wsErrors = new Counter('ws_errors');
const wsLatency = new Trend('ws_message_latency', true);
const wsConnectionTime = new Trend('ws_connection_time', true);
const wsSuccessRate = new Rate('ws_success_rate');

// Configuration
const BASE_URL = __ENV.TARGET || 'ws://localhost:8082';
const JWT_SECRET = __ENV.JWT_SECRET || 'test-secret-for-load-testing';

// Test scenarios for different load profiles
export const options = {
    scenarios: {
        // Ramp-up test
        ramp_up: {
            executor: 'ramping-vus',
            startVUs: 0,
            stages: [
                { duration: '2m', target: 100 },    // Warm up
                { duration: '5m', target: 1000 },   // Ramp to 1K
                { duration: '10m', target: 5000 },  // Ramp to 5K
                { duration: '20m', target: 10000 }, // Ramp to 10K
                { duration: '10m', target: 10000 }, // Hold at 10K
                { duration: '5m', target: 0 },      // Ramp down
            ],
            gracefulRampDown: '30s',
        },
        // Constant load test (for stability)
        // constant_load: {
        //     executor: 'constant-vus',
        //     vus: 5000,
        //     duration: '30m',
        // },
        // Spike test
        // spike: {
        //     executor: 'ramping-vus',
        //     startVUs: 1000,
        //     stages: [
        //         { duration: '1m', target: 1000 },
        //         { duration: '30s', target: 10000 }, // Spike
        //         { duration: '1m', target: 10000 },
        //         { duration: '30s', target: 1000 },  // Drop
        //         { duration: '2m', target: 1000 },
        //     ],
        // },
    },
    thresholds: {
        ws_success_rate: ['rate>0.95'],           // 95% success rate
        ws_message_latency: ['p(95)<500'],        // 95th percentile < 500ms
        ws_connection_time: ['p(95)<2000'],       // Connection < 2s
        ws_errors: ['count<100'],                 // Less than 100 errors
    },
};

// Generate a mock JWT token for testing
function generateTestToken(userId) {
    // In real tests, use proper JWT generation
    // This is a simplified mock for load testing
    const header = btoa(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
    const payload = btoa(JSON.stringify({
        sub: userId,
        exp: Math.floor(Date.now() / 1000) + 3600,
        iat: Math.floor(Date.now() / 1000),
    }));
    // Note: This won't validate - use real tokens in actual tests
    return `${header}.${payload}.mock_signature`;
}

// Main test function
export default function () {
    const userId = `user_${__VU}_${randomString(8)}`;
    const conversationId = `conv_${randomIntBetween(1, 1000)}`;
    const token = generateTestToken(userId);

    const url = `${BASE_URL}/ws?token=${token}`;
    const startTime = Date.now();

    const res = ws.connect(url, {}, function (socket) {
        const connectionTime = Date.now() - startTime;
        wsConnectionTime.add(connectionTime);
        wsConnections.add(1);

        let messagesSent = 0;
        let messagesReceived = 0;
        let lastSendTime = 0;

        socket.on('open', () => {
            wsSuccessRate.add(1);

            // Send authentication message
            socket.send(JSON.stringify({
                type: 'Authenticate',
                payload: { token: token }
            }));
        });

        socket.on('message', (data) => {
            messagesReceived++;
            wsMessagesReceived.add(1);

            try {
                const msg = JSON.parse(data);

                // Track latency for echo/response messages
                if (lastSendTime > 0 && (msg.type === 'message_sent' || msg.type === 'pong')) {
                    wsLatency.add(Date.now() - lastSendTime);
                }

                // Handle different message types
                switch (msg.type) {
                    case 'authenticated':
                        // Start sending messages after auth
                        break;
                    case 'error':
                        wsErrors.add(1);
                        console.error(`Error: ${msg.message}`);
                        break;
                }
            } catch (e) {
                // Non-JSON message
            }
        });

        socket.on('error', (e) => {
            wsErrors.add(1);
            wsSuccessRate.add(0);
            console.error(`WebSocket error: ${e}`);
        });

        socket.on('close', () => {
            wsConnections.add(-1);
        });

        // Simulate realistic user behavior
        socket.setInterval(() => {
            // Send a chat message every 5-15 seconds
            const msgType = randomIntBetween(1, 10);
            lastSendTime = Date.now();

            if (msgType <= 6) {
                // 60% - Regular message
                socket.send(JSON.stringify({
                    type: 'SendMessage',
                    payload: {
                        conversation_id: conversationId,
                        content: `Test message ${messagesSent} from ${userId}`,
                        content_type: 'Text',
                        client_message_id: `${userId}_${messagesSent}_${Date.now()}`,
                        mentions: [],
                    }
                }));
                messagesSent++;
                wsMessages.add(1);
            } else if (msgType <= 8) {
                // 20% - Typing indicator
                socket.send(JSON.stringify({
                    type: 'Typing',
                    payload: {
                        conversation_id: conversationId,
                        is_typing: true
                    }
                }));
            } else if (msgType === 9) {
                // 10% - Mark as read
                socket.send(JSON.stringify({
                    type: 'MarkRead',
                    payload: {
                        conversation_id: conversationId,
                        message_id: `msg_${randomIntBetween(1, 1000)}`
                    }
                }));
            } else {
                // 10% - Ping
                socket.send(JSON.stringify({ type: 'Ping' }));
            }
        }, randomIntBetween(5000, 15000));

        // Keep connection open for the test duration
        socket.setTimeout(() => {
            socket.close();
        }, 60000 * 5); // 5 minutes per VU session
    });

    check(res, {
        'WebSocket connection successful': (r) => r && r.status === 101,
    });

    // Small sleep between reconnection attempts
    sleep(randomIntBetween(1, 3));
}

// Lifecycle hooks
export function setup() {
    console.log(`Starting load test against ${BASE_URL}`);
    console.log(`Target: Simulate million-scale concurrent connections`);
    return { startTime: Date.now() };
}

export function teardown(data) {
    const duration = (Date.now() - data.startTime) / 1000;
    console.log(`Load test completed in ${duration}s`);
}
