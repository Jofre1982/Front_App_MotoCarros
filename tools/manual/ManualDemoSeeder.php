<?php

namespace Database\Seeders;

use App\Enums\DocumentStatus;
use App\Enums\DocumentType;
use App\Enums\DriverVerificationStatus;
use App\Enums\VehicleType;
use App\Models\Ad;
use App\Models\DriverDocument;
use App\Models\DriverProfile;
use App\Models\User;
use App\Models\Vehicle;
use Illuminate\Database\Seeder;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Storage;

/** Datos de ejemplo SOLO para tomar los pantallazos del manual. */
class ManualDemoSeeder extends Seeder
{
    public function run(): void
    {
        $pasajera = User::factory()->create([
            'name' => 'María Yucuna',
            'email' => 'pasajera@demo.co',
            'phone' => '+573101112233',
            'password' => Hash::make('demo12345'),
        ]);
        $pasajera->forceFill(['phone_verified_at' => now(), 'email_verified_at' => now()])->save();

        $conductor = User::factory()->driver()->create([
            'name' => 'Carlos Gaitán',
            'email' => 'conductor@demo.co',
            'phone' => '+573204445566',
            'password' => Hash::make('demo12345'),
        ]);
        $conductor->forceFill(['phone_verified_at' => now(), 'email_verified_at' => now()])->save();

        $perfil = DriverProfile::factory()->create([
            'user_id' => $conductor->id,
            'verification_status' => DriverVerificationStatus::Verified,
            'is_available' => false,
            'is_available_for_errands' => true,
            'latitude' => 3.8672,
            'longitude' => -67.9205,
            'location_updated_at' => now(),
            'subscription_ends_at' => now()->addMonth(),
        ]);
        Vehicle::factory()->create([
            'user_id' => $conductor->id,
            'plate' => 'XYZ12A',
            'type' => VehicleType::Motocarro,
        ]);

        $foto = file_get_contents(base_path('demo/moto.jpg'));
        foreach (DocumentType::cases() as $tipo) {
            $path = "driver-documents/{$perfil->id}/{$tipo->value}.jpg";
            Storage::disk('local')->put($path, $foto);
            DriverDocument::factory()->create([
                'driver_profile_id' => $perfil->id,
                'type' => $tipo,
                'path' => $path,
                'status' => DocumentStatus::Approved,
                'reviewed_at' => now(),
            ]);
        }

        if (class_exists(Ad::class)) {
            Storage::disk('local')->put('ads/drogueria.png', file_get_contents(base_path('demo/anuncio.png')));
            Ad::create([
                'business_name' => 'Droguería La 14',
                'message' => 'Domicilios gratis en todo el pueblo',
                'whatsapp' => '3001234567',
                'image_path' => 'ads/drogueria.png',
                'starts_on' => now('America/Bogota')->subDay()->toDateString(),
                'ends_on' => now('America/Bogota')->addMonth()->toDateString(),
            ]);
        }
    }
}
