import React, { useState, useEffect } from "react";
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import Image from 'next/image';
import { Button } from './ui/button';
import { CheckCircle2 } from 'lucide-react';
import { toast } from 'sonner';


export function About() {
    const [currentVersion, setCurrentVersion] = useState<string>('0.0.1');

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    const openExternal = (url: string) => {
        invoke('open_external_url', { url }).catch((error) => {
            console.error('Failed to open external link:', error);
            toast.error('Could not open the link');
        });
    };

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt="RECMeetily Logo"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                {/* <h1 className="text-xl font-bold text-gray-900">RECMeetily</h1> */}
                <span className="text-sm text-gray-500"> v{currentVersion}</span>
                <p className="text-medium text-gray-600 mt-1">
                    Real-time notes and summaries that never leave your machine.
                </p>
                <div className="mt-3">
                    <Button
                        onClick={() => openExternal('https://github.com/felipecortesp/RECMeetily/releases')}
                        variant="outline"
                        size="sm"
                        className="text-xs"
                    >
                        <CheckCircle2 className="h-3 w-3 mr-2" />
                        View macOS Releases
                    </Button>
                </div>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-gray-800">What makes RECMeetily different</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">Privacy-first</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">Your data & AI processing workflow can now stay within your premise. No cloud, no leaks.</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">Use Any Model</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">Prefer local open-source model? Great. Want to plug in an external API? Also fine. No lock-in.</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">Cost-Smart</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">Avoid pay-per-minute bills by running models locally (or pay only for the calls you choose).</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">Works everywhere</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">Google Meet, Zoom, Teams-online or offline.</p>
                    </div>
                </div>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-gray-200 text-center">
                <p className="text-xs text-gray-400">
                    RECMeetily · Open source (MIT)
                </p>
                <p className="mt-1 text-xs text-gray-500">
                    Based on{' '}
                    <button
                        type="button"
                        className="underline underline-offset-2 transition-colors hover:text-blue-500"
                        onClick={() => openExternal('https://github.com/Zackriya-Solutions/meeting-minutes')}
                    >
                        Meetily
                    </button>
                    {' '}by Zackriya Solutions and{' '}
                    <button
                        type="button"
                        className="underline underline-offset-2 transition-colors hover:text-blue-500"
                        onClick={() => openExternal('https://github.com/TylerBuza/Meetily-ActuallyFree')}
                    >
                        Meetily - Actually Free
                    </button>
                    {' '}by Tyler Buza
                </p>
                <p className="mt-1 text-xs text-gray-500">
                    Maintained by Felipe Cortes ·{' '}
                    <button
                        type="button"
                        className="underline underline-offset-2 transition-colors hover:text-blue-500"
                        onClick={() => openExternal('https://github.com/felipecortesp/RECMeetily')}
                    >
                        GitHub
                    </button>
                </p>
            </div>
        </div>

    )
}
